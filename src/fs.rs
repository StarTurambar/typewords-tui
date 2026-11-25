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

/// 去掉文本开头的一个 UTF-8 BOM（U+FEFF）。
///
/// Windows 记事本等工具会以「带 BOM 的 UTF-8」保存 JSON，而 `serde_json`
/// 不把 U+FEFF 当空白：不剥离就会把好档判成解析失败、进而隔离。只剥离开头
/// 的**一个字符**，不 `trim` 整段——正文里的空白与 U+FEFF 都有意义。
pub fn strip_utf8_bom(text: &str) -> &str {
    text.strip_prefix('\u{feff}').unwrap_or(text)
}

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
    write_atomic_bytes(path, text.as_bytes())
}

/// 原子写字节内容：与 `write_atomic` 走同一条路径，供不经过文本的调用方使用。
pub fn write_atomic_bytes(path: &Path, bytes: &[u8]) -> Result<()> {
    write_atomic_with(path, bytes, |_| {})
}

/// 导入 / 备份读入的大小上限：512 MiB。
///
/// 只约束「用户指定的外部文件」：`tw import-dict` 的词条文件与 `tw import-web`
/// 的备份。整份读进内存前先看 `metadata`，超限直接报「文件过大」，而不是等
/// 系统内存耗尽把进程杀掉。自家存档的备份 / 回滚走 `copy_atomic`，那里**不设**
/// 上限——限制它反而会挡住用户备份大存档。
pub const IMPORT_SIZE_LIMIT: u64 = 512 * 1024 * 1024;

/// 读取外部 JSON 文本；超过 `limit` 字节直接报错，不整读进内存。
///
/// 先 `metadata` 预检再整读。`metadata` 失败（文件不存在、权限不足）时继续
/// 交给 `read_to_string` 报真实错误；预检与读取之间文件变大不追（单用户本地
/// 工具，不值得为这个窗口引入流式读取）。
///
/// 错误信息只说大小、不带路径：两个导入入口都会用 `with_context` 补上
/// 「读取 ... 失败」的上下文，这里再拼一次会让同一个路径在错误链里出现两遍。
pub fn read_text_limited(path: &Path, limit: u64) -> Result<String> {
    if let Ok(metadata) = fs::metadata(path)
        && metadata.len() > limit
    {
        return Err(anyhow!(
            "文件过大：{}，上限 {}",
            human_size(metadata.len()),
            human_size(limit)
        ));
    }
    // IO 错误原样交给调用方补上下文（两个导入入口分别说「读取词库失败」与
    // 「读取 <path> 失败」）。
    let text = fs::read_to_string(path)?;
    Ok(text)
}

/// 把字节数写成给人看的大小：≥ 1 MiB 用 MiB、≥ 1 KiB 用 KiB，否则字节。
///
/// 只用于「文件过大」这类提示。能用一个十进制精确表示时只写换算值
/// （`512 MiB` / `1.5 KiB`）；不能时保留字节数再附近似值
/// （`536870913 字节（约 512.0 MiB）`）——否则「比上限多 1 字节」会被四舍五入
/// 成和上限一模一样的 `512.0 MiB`。
fn human_size(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = 1024 * 1024;
    let (unit, scale) = if bytes >= MIB {
        ("MiB", MIB)
    } else if bytes >= KIB {
        ("KiB", KIB)
    } else {
        return format!("{bytes} 字节");
    };
    // 以 0.1 为单位判断能否精确表示，避免浮点比较。
    let tenths = bytes.saturating_mul(10);
    if !tenths.is_multiple_of(scale) {
        return format!(
            "{bytes} 字节（约 {:.1} {unit}）",
            bytes as f64 / scale as f64
        );
    }
    let value = tenths / scale;
    if value.is_multiple_of(10) {
        format!("{} {unit}", value / 10)
    } else {
        format!("{}.{} {unit}", value / 10, value % 10)
    }
}

/// 原子复制：读出 `from` 的全部字节，再用 `write_atomic_bytes` 写到 `to`。
///
/// `fs::copy` 是「打开目标 → 截断 → 逐块写」，中途失败会留下半截目标文件；
/// 把备份还原成存档时正好相反——需要的是「要么是完整旧内容，要么原样不动」。
/// 存档都是几百 KB 到几 MB 的 JSON，整份读进内存的代价可以忽略。
///
/// 权限位按源文件复制（`fs::copy` 也这样做）：用户手工收紧过 0600 的存档，
/// 备份与回滚不应把它放宽成 umask 默认值。复制权限失败不影响已写好的内容。
///
/// Windows 例外：那里的 `Permissions` 只有只读位，而只读位会让
/// `MoveFileEx` 拒绝覆盖目标（见 [replace_file]）——把它复制给 `.bak` 就等于
/// 让「存档 → 备份 → 回滚」之间传染只读属性，下一次保存或回滚必然失败。
/// 因此 Windows 上不复制权限，目标保持临时文件的普通属性。
pub fn copy_atomic(from: &Path, to: &Path) -> Result<()> {
    let bytes = fs::read(from).with_context(|| format!("读取文件失败: {}", from.display()))?;
    write_atomic_bytes(to, &bytes)?;
    #[cfg(not(windows))]
    if let Ok(metadata) = fs::metadata(from) {
        let _ = fs::set_permissions(to, metadata.permissions());
    }
    Ok(())
}

/// `write_atomic` 的实现；`before_rename` 只给单测注入「临时文件被外部删除」
/// 这类故障用，生产路径传空闭包。
fn write_atomic_with(path: &Path, bytes: &[u8], before_rename: impl FnOnce(&Path)) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("创建目录失败: {}", parent.display()))?;
    }
    let (tmp, mut file) = create_temp(path)?;
    let written = file
        .write_all(bytes)
        .with_context(|| format!("写入临时文件失败: {}", tmp.display()));
    // fsync 失败只说明「不保证已落盘」，本次进度仍然该保存，因此不阻断。
    let _ = file.sync_all();
    drop(file);
    if let Err(err) = written {
        remove_temp(&tmp);
        return Err(err);
    }

    before_rename(&tmp);
    if let Err(err) = replace_file(&tmp, path) {
        // 临时文件没被换上去就没有价值，顺手清理，避免数据目录里堆垃圾。
        remove_temp(&tmp);
        return Err(anyhow::Error::new(err)
            .context(format!("替换存档失败: {}{REPLACE_HINT}", path.display())));
    }
    // rename 本身只是目录项的一次改动：不同步父目录时，断电后可能出现
    // 「旧文件还在、新文件没了」——内容仍是完整的（不会半截），但本次保存
    // 整份丢失。与上面的文件 fsync 同一口径：失败只说明不保证落盘，不阻断。
    sync_parent_dir(path);
    Ok(())
}

/// rename 最终失败时补在上下文里的提示。
///
/// Windows 上「拒绝访问」最可能来自同步盘 / 杀毒软件的瞬时占用，或清不掉的
/// 只读属性；Unix 的 rename 没有这两种歧义，不加提示以免误导。
#[cfg(windows)]
const REPLACE_HINT: &str = "（文件可能被同步盘或杀毒软件占用，也可能只读属性无法清除；请稍后重试）";
#[cfg(not(windows))]
const REPLACE_HINT: &str = "";

/// Windows 上 rename 遇到瞬时占用时的退避间隔（毫秒）。
///
/// 杀毒扫描、搜索索引与同步盘会短暂地以不共享方式持有目标文件，此时
/// `MoveFileEx` 返回「拒绝访问」。总退避 70 ms：足够跨过一次扫描窗口，
/// 又短到不会被用户察觉为卡顿。
#[cfg(windows)]
const RENAME_BACKOFF_MS: [u64; 3] = [10, 20, 40];

/// 用 `tmp` 覆盖 `path`。
///
/// Windows 的 rename 是 `MoveFileEx(MOVEFILE_REPLACE_EXISTING)`：目标带只读
/// 属性或正被别的进程以不共享方式打开时都返回「拒绝访问」（os error 5），
/// 而 POSIX 的 `rename(2)` 只看目录写权限——同一份代码在 Unix 上能过、在
/// Windows 上必失败。这里先清掉目标的只读属性，再对瞬时占用做几次小退避
/// 重试；Unix 保持一次 `rename`，不改变原有语义。
///
/// 清掉的只读属性在最终失败时会被恢复：保存没成功，旧存档的内容和属性都该
/// 原样不动。占用导致的失败是个例外——恢复本身也要打开同一个文件，会被同一
/// 把占用挡住，此时尽力而为（见 [restore_readonly]）。
#[cfg(windows)]
fn replace_file(tmp: &Path, path: &Path) -> std::io::Result<()> {
    let cleared_readonly = clear_readonly(path);
    let mut attempt = 0;
    loop {
        match fs::rename(tmp, path) {
            Ok(()) => return Ok(()),
            Err(err) if is_transient_rename_error(&err) && attempt < RENAME_BACKOFF_MS.len() => {
                let backoff = RENAME_BACKOFF_MS[attempt];
                std::thread::sleep(std::time::Duration::from_millis(backoff));
                attempt += 1;
            }
            Err(err) => {
                if cleared_readonly {
                    restore_readonly(path);
                }
                return Err(err);
            }
        }
    }
}

/// 目标存在且带只读属性时清掉它，返回「是否真的清过」。
///
/// Windows 的 rename 拒绝覆盖只读目标，不清就相当于「给存档加只读 = 每次
/// 保存都失败」。清属性失败不在这里报错——rename 自己会给出带提示的错误，
/// 此时返回 `false`，失败分支也就没有需要恢复的东西。
#[cfg(windows)]
fn clear_readonly(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    let mut permissions = metadata.permissions();
    if !permissions.readonly() {
        return false;
    }
    // 只读位是 Windows 上 `Permissions` 唯一表达的属性；这里的 `false` 是
    // 清除「拒绝覆盖」这个障碍，不是把 Unix 的权限位放宽。
    #[allow(clippy::permissions_set_readonly_false)]
    {
        permissions.set_readonly(false);
    }
    fs::set_permissions(path, permissions).is_ok()
}

/// 保存最终失败时把刚清掉的只读属性恢复回去。
///
/// 只读属性是用户「别动这个文件」的显式意图，而保存失败时旧存档的内容原样
/// 保留——属性也该如此，否则一次失败的保存会悄悄把防误删的保护撤掉。
///
/// 尽力而为：失败原因若是文件被别的进程独占，恢复所需的打开会被同一把占用
/// 挡住，此时保持现状（用户拿到的是带「被占用」提示的错误）。
#[cfg(windows)]
fn restore_readonly(path: &Path) {
    let Ok(metadata) = fs::metadata(path) else {
        return;
    };
    let mut permissions = metadata.permissions();
    if permissions.readonly() {
        return;
    }
    permissions.set_readonly(true);
    let _ = fs::set_permissions(path, permissions);
}

/// 是否值得重试的 rename 错误：5（`ERROR_ACCESS_DENIED`，只读 / 被占用）与
/// 32（`ERROR_SHARING_VIOLATION`）都会在持有者放手后自行消失。
#[cfg(windows)]
fn is_transient_rename_error(err: &std::io::Error) -> bool {
    err.kind() == ErrorKind::PermissionDenied || err.raw_os_error() == Some(32)
}

/// Unix 上 `rename` 不会因为目标被打开或带只读位而失败，保持一次调用。
#[cfg(not(windows))]
fn replace_file(tmp: &Path, path: &Path) -> std::io::Result<()> {
    fs::rename(tmp, path)
}

/// 在 rename 之后同步父目录，让「新名字已经生效」这件事也落盘。
///
/// 只对 Unix 有意义：Windows 没有目录 fsync 的语义（对目录句柄调
/// FlushFileBuffers 不受支持），对应分支直接编译掉。目录打不开（权限、
/// 路径已被删）时静默跳过，理由同文件 fsync。
#[cfg(unix)]
fn sync_parent_dir(path: &Path) {
    let Some(parent) = path.parent() else {
        return;
    };
    // 相对路径的 parent 是空路径，代表当前目录。
    let parent = if parent.as_os_str().is_empty() {
        Path::new(".")
    } else {
        parent
    };
    if let Ok(dir) = File::open(parent) {
        let _ = dir.sync_all();
    }
}

/// Windows 没有目录 fsync 的概念，保持原样。
#[cfg(not(unix))]
fn sync_parent_dir(_path: &Path) {}

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
        .with_context(|| {
            // 打不开锁文件时 TUI 直接拒绝启动，而五份存档其实都还可读。
            // 最常见的原因是「曾用管理员 / sudo 跑过一次，锁文件属于别的
            // 账号」，其次才是整个目录不可写；文案要能直接照着做。
            format!(
                "打开数据目录锁失败: {}（若曾用管理员 / sudo 运行过 tw，删除该文件或修正属主后重试；数据目录不可写时用 --home 指定可写目录）",
                path.display()
            )
        })?;
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

    /// Unix 分支的编译与边界覆盖：相对文件名的 parent 是空路径，必须按
    /// 当前目录处理而不是 panic。断电语义无法在测试里模拟，由代码审阅与
    /// Unix CI 的编译覆盖把关。
    #[cfg(unix)]
    #[test]
    fn sync_parent_dir_handles_a_relative_file_name() {
        sync_parent_dir(Path::new("dict.json"));
        let dir = tempfile::tempdir().unwrap();
        sync_parent_dir(&dir.path().join("dict.json"));
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
        let err = write_atomic_with(&target, b"new", |tmp| {
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

    /// 外部导入文件超限时，在整读进内存之前就报「文件过大」。
    ///
    /// 阈值做成参数，单测用 8 字节验证，不需要造 GB 级临时文件。
    #[test]
    fn read_text_limited_rejects_a_file_over_the_limit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("words.json");
        fs::write(&path, "12345678").unwrap();

        assert_eq!(
            read_text_limited(&path, 8).expect("正好等于上限应可读"),
            "12345678"
        );
        let err = read_text_limited(&path, 7).expect_err("超过上限应报错");
        let message = err.to_string();
        assert!(message.contains("文件过大"), "{message}");
        assert!(message.contains("8 字节"), "应说明实际大小: {message}");
        assert!(
            !message.contains(&path.display().to_string()),
            "路径由调用方补上下文，这里不该再拼一遍: {message}"
        );
    }

    /// 大文件用 MiB 显示：512 MiB 上限不该写成 536870912 字节。
    ///
    /// 刚好越过上限 1 字节时不能四舍五入成「512.0 MiB」（看起来等于上限），
    /// 要保留精确字节数。
    #[test]
    fn human_size_prefers_mib_for_large_files() {
        assert_eq!(human_size(IMPORT_SIZE_LIMIT), "512 MiB");
        assert_eq!(human_size(514 * 1024 * 1024), "514 MiB");
        assert_eq!(human_size(1536), "1.5 KiB");
        assert_eq!(human_size(8), "8 字节");
        assert_eq!(
            human_size(IMPORT_SIZE_LIMIT + 1),
            "536870913 字节（约 512.0 MiB）"
        );
    }

    #[test]
    fn copy_atomic_restores_the_source_bytes_without_leaving_a_temp_file() {
        // 回滚走的就是这条路径：目标先被改坏，再从备份整份还原。
        let dir = tempfile::tempdir().unwrap();
        let backup = dir.path().join("dict.json.bak-20260101-120000.json");
        let target = dir.path().join("dict.json");
        fs::write(&backup, "{\"val\":{},\"version\":4}").unwrap();
        fs::write(&target, "{ 导入后的内容 }").unwrap();

        copy_atomic(&backup, &target).unwrap();

        assert_eq!(
            fs::read_to_string(&target).unwrap(),
            "{\"val\":{},\"version\":4}"
        );
        assert!(temp_files(dir.path()).is_empty(), "回滚不应留下临时文件");
    }

    /// `fs::copy` 会保留源文件权限；原子复制也必须如此。
    #[cfg(unix)]
    #[test]
    fn copy_atomic_preserves_source_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let from = dir.path().join("dict.json");
        let to = dir.path().join("dict.json.bak-20260101-120000.json");
        fs::write(&from, "{\"val\":{},\"version\":4}").unwrap();
        fs::set_permissions(&from, fs::Permissions::from_mode(0o600)).unwrap();

        copy_atomic(&from, &to).unwrap();

        let mode = fs::metadata(&to).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "备份/回滚不应放宽源存档的权限");
    }

    /// Windows 的 rename 拒绝覆盖只读目标（`MoveFileEx` 语义）。不清属性时
    /// 「给 dict.json 加只读防误删」会让每一次保存都失败，进度只留在内存里。
    #[cfg(windows)]
    #[test]
    fn write_atomic_clears_a_readonly_target() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("dict.json");
        write_atomic(&target, "old").unwrap();
        let mut permissions = fs::metadata(&target).unwrap().permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&target, permissions).unwrap();

        write_atomic(&target, "new").expect("只读目标不应让保存失败");

        assert_eq!(fs::read_to_string(&target).unwrap(), "new");
        assert!(
            !fs::metadata(&target).unwrap().permissions().readonly(),
            "换上去的新文件不应带着只读属性"
        );
        assert!(temp_files(dir.path()).is_empty(), "写完不应留下临时文件");
    }

    /// 目标被别的句柄以不共享方式打开时 rename 返回「拒绝访问」；占用通常在
    /// 几十毫秒内消失，保存要能靠退避重试自己恢复，而不是把这次进度丢掉。
    #[cfg(windows)]
    #[test]
    fn write_atomic_retries_while_the_target_is_briefly_held() {
        use std::os::windows::fs::OpenOptionsExt;
        use std::time::Duration;

        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("dict.json");
        write_atomic(&target, "old").unwrap();

        let holder = OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(0)
            .open(&target)
            .expect("模拟另一个进程独占目标");
        let release = thread::spawn(move || {
            thread::sleep(Duration::from_millis(20));
            drop(holder);
        });

        write_atomic(&target, "new").expect("瞬时占用应靠重试成功");
        release.join().unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "new");
        assert!(
            temp_files(dir.path()).is_empty(),
            "重试成功后不应留下临时文件"
        );
    }

    /// 占用一直不消失时最终仍要报错，但文案要点出「被占用」这件事，并保持
    /// 原文件不变。
    #[cfg(windows)]
    #[test]
    fn write_atomic_reports_a_held_target_with_an_actionable_error() {
        use std::os::windows::fs::OpenOptionsExt;

        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("dict.json");
        write_atomic(&target, "old").unwrap();

        let holder = OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(0)
            .open(&target)
            .expect("模拟另一个进程独占目标");
        let err = write_atomic(&target, "new").expect_err("持续占用最终必须报错");
        let message = format!("{err:#}");

        assert!(message.contains("替换存档失败"), "{message}");
        assert!(message.contains("占用"), "文案应点出占用: {message}");
        assert!(
            message.contains("稍后重试"),
            "文案应给出可操作建议: {message}"
        );

        drop(holder);
        assert_eq!(
            fs::read_to_string(&target).unwrap(),
            "old",
            "失败时正式文件应保持原内容"
        );
        assert!(temp_files(dir.path()).is_empty(), "失败要清掉本次临时文件");
    }

    /// 保存最终失败时，旧存档要原样不动——内容如此，用户加的只读属性也如此。
    /// 覆盖前清属性是为了让 rename 能过；没换成新文件就该把属性还回去，否则
    /// 一次失败会把「防误删」的保护悄悄撤掉。
    ///
    /// 用「本次临时文件被外部删掉」制造最终失败：这条路径不占着目标，恢复
    /// 属性不会被占用挡住（被独占时恢复只能尽力而为，那是另一回事）。
    #[cfg(windows)]
    #[test]
    fn write_atomic_restores_the_readonly_target_when_the_rename_fails() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("dict.json");
        write_atomic(&target, "old").unwrap();
        let mut permissions = fs::metadata(&target).unwrap().permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&target, permissions).unwrap();

        let err = write_atomic_with(&target, b"new", |tmp| {
            fs::remove_file(tmp).expect("模拟外部删除本次的临时文件");
        })
        .expect_err("临时文件被删后 rename 必须失败");
        assert!(err.to_string().contains("替换存档失败"), "{err}");

        assert!(
            fs::metadata(&target).unwrap().permissions().readonly(),
            "保存失败后应把只读属性恢复回去"
        );
        assert_eq!(
            fs::read_to_string(&target).unwrap(),
            "old",
            "失败时正式文件应保持原内容"
        );
        assert!(temp_files(dir.path()).is_empty(), "失败要清掉本次临时文件");
    }

    /// Windows 的 `Permissions` 只有只读位：把它从源复制给 `.bak`，再回滚到
    /// 存档，只读位就在「存档 → 备份 → 回滚」之间传染，下一次保存 / 回滚失败。
    #[cfg(windows)]
    #[test]
    fn copy_atomic_does_not_propagate_the_readonly_attribute() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("dict.json");
        let backup = dir.path().join("dict.json.bak-20260101-120000.json");
        fs::write(&archive, "{\"val\":{},\"version\":4}").unwrap();
        let mut permissions = fs::metadata(&archive).unwrap().permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&archive, permissions).unwrap();

        copy_atomic(&archive, &backup).expect("只读源文件也应能备份");
        assert!(
            !fs::metadata(&backup).unwrap().permissions().readonly(),
            "备份不应继承只读属性"
        );

        copy_atomic(&backup, &archive).expect("回滚不应被只读位挡住");
        assert!(
            !fs::metadata(&archive).unwrap().permissions().readonly(),
            "回滚后的存档不应继承只读属性"
        );
        assert_eq!(
            fs::read_to_string(&archive).unwrap(),
            "{\"val\":{},\"version\":4}"
        );
    }

    /// 锁文件打不开时 TUI 会拒绝启动；错误必须给出「删掉锁文件 / 修属主 /
    /// 换 --home」这类能直接照做的动作，而不是只说一句「打开失败」。
    #[test]
    fn lock_open_failure_explains_how_to_recover() {
        let dir = tempfile::tempdir().unwrap();
        // 用同名目录让 `.tw.lock` 无法作为文件打开：目录只读、属主不对等
        // 真实故障都走同一条错误路径。
        fs::create_dir(dir.path().join(LOCK_FILE_NAME)).unwrap();

        let err = try_lock_dir(dir.path()).expect_err("锁文件无法打开时必须报错");
        let message = format!("{err:#}");

        assert!(message.contains("打开数据目录锁失败"), "{message}");
        assert!(
            message.contains("删除该文件"),
            "应说明怎么清掉坏锁: {message}"
        );
        assert!(
            message.contains("--home"),
            "应给出替代的数据目录: {message}"
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
