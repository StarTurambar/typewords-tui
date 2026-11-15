//! 底层的文件原语：原子写与数据目录互斥锁。
//!
//! 单独成模块是为了让分层保持单向：`domain` 不依赖 `store`，而 `store`
//! 依赖 `domain`。原子写是两边都要用的东西（存档层写 JSON，领域层写自建
//! 词库与用户词库目录），放在这里就不会形成 `domain ↔ store` 的环。
//!
//! 数据目录锁也放在这里：它同样只用 `std`，并且与原子写解决的是同一类问题
//! ——「固定名临时文件 + 无锁」的组合会让两个 `tw` 进程互相踩掉对方的临时
//! 文件，还会在退出时整份覆盖对方刚存好的存档。

use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, Result, anyhow};

/// 临时文件名里的单调计数：每次写入取一个新值，保证同一进程内的并发写
/// （例如存档与词库目录同时落盘）不会共用同一个临时文件。
static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

/// 临时文件撞名后的重试上限。
///
/// 正常情况一次就够：名字里有 PID，撞名只可能来自「同 PID 的历史残留」
/// （PID 复用）或共享同一数据目录的其它 PID 命名空间。
const TEMP_ATTEMPTS: u32 = 16;

/// 数据目录锁文件名（位于 `<home>` 下）。
pub const LOCK_FILE_NAME: &str = ".tw.lock";

/// 原子写：先写同目录的临时文件，`fsync` 后 `rename` 覆盖目标。
///
/// 同目录保证 `rename` 在同一文件系统内（跨文件系统的 `rename` 可能失败）；
/// `fsync` 保证内容真的落盘，断电时不会留下半个文件。
///
/// 临时文件名带进程号与进程内单调计数（`.<文件名>.tw-<pid>-<n>.tmp`）并以
/// `create_new` 独占创建。固定名（曾经是 `.<文件名>.tmp`）在两个进程同时
/// 写同一份存档时会互相截断：先写完的那个 `rename` 可能把对方的半截内容
/// 换到正式文件上，或因临时文件被对方删除而失败（表现为「保存词典失败」）。
pub fn write_atomic(path: &Path, text: &str) -> Result<()> {
    write_atomic_with(path, text, |_| {})
}

/// `write_atomic` 的实现；`before_rename` 只给单测注入「临时文件被外部删除」
/// 这类故障用，生产路径传空闭包。
fn write_atomic_with(path: &Path, text: &str, before_rename: impl FnOnce(&Path)) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("创建目录失败: {}", parent.display()))?;
    }
    let (tmp, mut file) = create_temp(path)?;
    let written = file
        .write_all(text.as_bytes())
        .with_context(|| format!("写入临时文件失败: {}", tmp.display()));
    // fsync 失败只说明「不保证已落盘」，本次进度仍然该保存，因此不阻断。
    let _ = file.sync_all();
    drop(file);
    if let Err(err) = written {
        remove_temp(&tmp);
        return Err(err);
    }

    before_rename(&tmp);
    if let Err(err) = fs::rename(&tmp, path) {
        // 临时文件没被换上去就没有价值，顺手清理，避免数据目录里堆垃圾。
        remove_temp(&tmp);
        return Err(err).with_context(|| format!("替换存档失败: {}", path.display()));
    }
    Ok(())
}

/// 建一个本次写入独占的临时文件，返回路径与句柄。
///
/// 用 `create_new` 而不是 `create`：万一撞到同名残留（PID 复用留下的旧
/// 临时文件），换下一个序号，绝不截断一个可能属于别人的临时文件。
fn create_temp(path: &Path) -> Result<(PathBuf, File)> {
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "state.json".to_string());
    let pid = std::process::id();
    for _ in 0..TEMP_ATTEMPTS {
        let seq = TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
        let candidate = path.with_file_name(format!(".{file_name}.tw-{pid}-{seq}.tmp"));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => return Ok((candidate, file)),
            Err(err) if err.kind() == ErrorKind::AlreadyExists => continue,
            Err(err) => {
                return Err(anyhow!("写入临时文件失败: {}: {err}", candidate.display()));
            }
        }
    }
    Err(anyhow!(
        "写入临时文件失败: {}（已重试 {TEMP_ATTEMPTS} 次仍撞名）",
        path.display()
    ))
}

/// 尽力清理临时文件；失败不覆盖原始错误。
fn remove_temp(tmp: &Path) {
    let _ = fs::remove_file(tmp);
}

/// 数据目录的独占锁（`<home>/.tw.lock`）。
///
/// 进程退出或守卫被 drop 时由操作系统释放；进程崩溃也会随句柄关闭释放，
/// 因此锁文件不会「卡死」下一次启动。锁文件本身不删除——删除一个正在被
/// 别人持锁的文件会制造经典的「两把锁」竞态。
///
/// 文件内容只记录持锁进程的 PID，供人工排障参考；判断是否被占用永远以
/// 操作系统锁为准（Windows 上整区间加锁后，其它句柄连读取都会被拒）。
#[derive(Debug)]
pub struct DirLock {
    file: File,
    path: PathBuf,
}

impl DirLock {
    /// 锁文件路径（`<home>/.tw.lock`）。
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for DirLock {
    fn drop(&mut self) {
        // 显式解锁；即使它失败，关闭句柄也会释放锁。
        let _ = self.file.unlock();
    }
}

/// 尝试独占 `dir` 的数据目录锁：已被另一个实例持有时返回错误。
///
/// 用非阻塞的 `try_lock` 而不是阻塞的 `lock`：抢不到锁就立刻告诉用户
/// 「另一个实例正在使用该数据目录」并退出，不挂在那里等别人关窗口。
pub fn try_lock_dir(dir: &Path) -> Result<DirLock> {
    let path = dir.join(LOCK_FILE_NAME);
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
        .with_context(|| format!("打开数据目录锁失败: {}", path.display()))?;
    match file.try_lock() {
        Ok(()) => {}
        Err(TryLockError::WouldBlock) => {
            return Err(anyhow!(
                "另一个实例正在使用该数据目录（{}）：请先退出那个 tw 进程，或用 --home 换一个数据目录",
                dir.display()
            ));
        }
        Err(TryLockError::Error(err)) => {
            return Err(err).with_context(|| format!("锁定数据目录失败: {}", path.display()));
        }
    }
    record_pid(&file);
    Ok(DirLock { file, path })
}

/// 把持锁进程的 PID 写进锁文件（仅供排障；失败不影响持锁）。
fn record_pid(file: &File) {
    let text = format!("{}\n", std::process::id());
    let handle = file;
    let _ = file.set_len(0).and_then(|()| {
        let mut handle = handle;
        handle.write_all(text.as_bytes())?;
        handle.flush()
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    /// 目录里残留的临时文件（含旧版固定名与新版唯一名）。
    fn temp_files(dir: &Path) -> Vec<String> {
        fs::read_dir(dir)
            .expect("读取目录")
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .filter(|name| name.ends_with(".tmp"))
            .collect()
    }

    #[test]
    fn write_atomic_replaces_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("dict.json");
        write_atomic(&target, "first").unwrap();
        write_atomic(&target, "second").unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "second");
        assert!(temp_files(dir.path()).is_empty(), "写完不应留下临时文件");
    }

    #[test]
    fn legacy_fixed_temp_file_is_never_reused() {
        // 旧版本或另一个进程可能留下固定名临时文件；新写入必须用自己独占的
        // 名字，绝不能把别人的半截内容 rename 到正式文件上。
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("dict.json");
        let legacy = dir.path().join(".dict.json.tmp");
        fs::write(&legacy, "{ legacy half written").unwrap();

        write_atomic(&target, "{\"ok\":true}").unwrap();

        assert_eq!(fs::read_to_string(&target).unwrap(), "{\"ok\":true}");
        assert_eq!(
            fs::read_to_string(&legacy).unwrap(),
            "{ legacy half written",
            "残留的固定名临时文件不属于本次写入，不应被复用或删除"
        );
    }

    #[test]
    fn external_removal_of_temp_file_errors_instead_of_writing_wrong_content() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("practice-word.json");
        write_atomic(&target, "old").unwrap();

        // 模拟最坏的外部干扰：本次写入的临时文件被删掉，同时别人在**旧版固定名**
        // 上放了一份半截内容。旧实现（固定名 .practice-word.json.tmp）会把这半截
        // 内容 rename 成正式存档；新实现只认自己独占的名字，rename 失败并保留原文。
        let legacy = dir.path().join(".practice-word.json.tmp");
        let err = write_atomic_with(&target, "new", |tmp| {
            fs::remove_file(tmp).expect("模拟外部删除本次的临时文件");
            fs::write(&legacy, "{ 半截外来内容 }").expect("模拟外部占用旧固定名");
        })
        .expect_err("临时文件被换掉后 rename 必须失败，而不是写入别人的内容");

        assert!(err.to_string().contains("替换存档失败"), "{err}");
        assert_eq!(
            fs::read_to_string(&target).unwrap(),
            "old",
            "失败时正式文件应保持原内容"
        );
        assert_eq!(
            temp_files(dir.path()),
            vec![".practice-word.json.tmp".to_string()],
            "本次写入的临时文件要清掉，外部放进去的文件不归本次管"
        );
        assert_eq!(
            fs::read_to_string(&legacy).unwrap(),
            "{ 半截外来内容 }",
            "外来文件不能被消费或删除"
        );
    }

    #[test]
    fn concurrent_writes_leave_one_complete_content() {
        // 同一进程内多线程写同一个存档：每次写入独占一个临时文件，最终内容
        // 必须恰好是某一次写入的完整内容，不能是两份内容拼出来的残片。
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("dict.json");
        let payloads: Vec<String> = (0..4)
            .map(|index| {
                format!(
                    "{{\"writer\":{index},\"pad\":\"{}\"}}",
                    "x".repeat(16 * 1024)
                )
            })
            .collect();

        let handles: Vec<_> = payloads
            .iter()
            .cloned()
            .map(|text| {
                let target = target.clone();
                thread::spawn(move || {
                    for _ in 0..25 {
                        write_atomic(&target, &text).expect("并发写不应失败");
                    }
                })
            })
            .collect();
        for handle in handles {
            handle.join().expect("写入线程不应 panic");
        }

        let final_text = fs::read_to_string(&target).unwrap();
        assert!(
            payloads.contains(&final_text),
            "并发写只应留下某一次写入的完整内容"
        );
        assert!(temp_files(dir.path()).is_empty(), "并发写不应留下临时文件");
    }

    #[test]
    fn temp_file_names_are_unique_and_carry_pid() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("dict.json");
        let (first, first_file) = create_temp(&target).unwrap();
        let (second, second_file) = create_temp(&target).unwrap();
        assert_ne!(first, second, "两次写入的临时文件必须不同名");

        let name = first.file_name().unwrap().to_string_lossy().to_string();
        assert!(name.starts_with(".dict.json.tw-"), "{name}");
        assert!(name.contains(&std::process::id().to_string()), "{name}");
        assert!(name.ends_with(".tmp"), "{name}");

        drop((first_file, second_file));
        remove_temp(&first);
        remove_temp(&second);
    }

    #[test]
    fn dir_lock_is_exclusive_until_dropped() {
        // Windows 的 LockFileEx 按句柄记账、Linux/macOS 的 flock 按打开文件描述
        // 记账，所以同一进程的两个句柄同样互斥；真正的跨进程保证由
        // tests/data_dir_lock.rs 里起真实子进程的用例把关。
        let dir = tempfile::tempdir().unwrap();
        let lock = try_lock_dir(dir.path()).unwrap();
        assert_eq!(lock.path(), dir.path().join(LOCK_FILE_NAME));

        let busy = try_lock_dir(dir.path()).unwrap_err();
        assert!(
            busy.to_string().contains("另一个实例正在使用该数据目录"),
            "{busy}"
        );

        drop(lock);
        // 锁文件记录持锁进程的 PID；Windows 上被锁区间连读取都会被拒，
        // 因此只在释放后读（内容仍保留）。
        let pid = fs::read_to_string(dir.path().join(LOCK_FILE_NAME)).unwrap();
        assert_eq!(pid.trim(), std::process::id().to_string());

        let again = try_lock_dir(dir.path()).expect("释放后应能重新取锁");
        drop(again);
    }
}
