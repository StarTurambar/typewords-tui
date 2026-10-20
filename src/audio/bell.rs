//! 终端响铃。
//!
//! 上游的键音与对错音是 mp3/wav；终端没有统一的播放通道，这里用 BEL 控制字符
//! 代替：零延迟、零资源、不需要外部播放器，代价是所有音听起来一样。

use std::io::Write;

/// 让终端响一声。
pub fn ring() {
    let mut stdout = std::io::stdout();
    // BEL 是控制字符，不会移动光标，也不会破坏 ratatui 的绘制
    if stdout.write_all(b"\x07").is_ok() {
        let _ = stdout.flush();
    }
}
