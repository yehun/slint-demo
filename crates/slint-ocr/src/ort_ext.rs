//! onnxruntime(ort) 统一入口: 本 crate 直接依赖 `ort`, 把动态库探测/初始化与会话构建
//! 收敛到这一处(替代 yehun-slint 的 `ort-onnx` 共享 crate, 让本 crate 自包含)。
//!
//! 消费者经本模块使用 `ort` 本体与 `build_session_from_bytes` / `ensure` / `Threads`:
//! ```ignore
//! use crate::ort_ext::{build_session_from_bytes, ensure, ort, Threads};
//! use crate::ort_ext::ort::{session::Session, value::Tensor};
//! ```

use std::path::Path;
use std::sync::OnceLock;

use anyhow::{anyhow, Result};
use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;

/// re-export ort 本体: 消费者经此使用 Session/Tensor/inputs! 等全部 API
pub use ort;

/// 会话 intra-op 线程数策略(对齐 yehun-slint 约定)
#[derive(Clone, Copy, Debug)]
pub enum Threads {
    /// 逐框串行调用(rec/cls): 一次只算一小块, 多线程调度开销大于收益, 固定 1 线程
    Serial,
    /// 一次算一大块(det): 取全局预算的一半(上限 4)
    Auto,
    /// 调用方显式指定
    Fixed(usize),
}

impl Threads {
    fn count(self) -> usize {
        match self {
            Threads::Serial => 1,
            Threads::Auto => {
                let n = std::thread::available_parallelism()
                    .map(|n| n.get())
                    .unwrap_or(2);
                (n / 2).clamp(1, 4)
            }
            Threads::Fixed(n) => n.max(1),
        }
    }
}

/// 加载 onnxruntime 动态库(Android 编译期链接模式下为 no-op)。只做一次(缓存含失败结果)
pub fn ensure() -> Result<(), String> {
    static RESULT: OnceLock<Result<(), String>> = OnceLock::new();
    RESULT
        .get_or_init(|| {
            #[cfg(target_os = "android")]
            {
                Ok(())
            }

            #[cfg(not(target_os = "android"))]
            {
                let path = find_library_path().ok_or_else(|| {
                    "未找到 onnxruntime 动态库(libonnxruntime.so / onnxruntime.dll / \
                     libonnxruntime.dylib)。请放到程序目录或 ./lib 目录, 或设置 ORT_DYLIB_PATH"
                        .to_string()
                })?;
                ort::init_from(&path)
                    .map(|_| ())
                    .map_err(|e| format!("加载 onnxruntime 库 {path:?} 失败: {e}"))
            }
        })
        .clone()
}

/// 从内存加载 ONNX 模型并构建会话(图优化 Level1 + 按策略配线程)
pub fn build_session_from_bytes(bytes: &[u8], threads: Threads) -> Result<Session> {
    degrade_on_probe_failure();
    let n = threads.count();
    let mut builder = Session::builder().map_err(|e| anyhow!("创建 ONNX 会话失败: {e}"))?;
    builder = builder
        .with_optimization_level(GraphOptimizationLevel::Level1)
        .map_err(|e| anyhow!("设置优化级别失败: {e}"))?;
    builder = builder
        .with_intra_threads(n)
        .map_err(|e| anyhow!("设置推理线程数失败: {e}"))?;
    builder
        .commit_from_memory(bytes)
        .map_err(|e| anyhow!("加载 ONNX 模型失败: {e}"))
}

/// 绕过引擎入口直接建会话时, 没人显式调过 [`ensure`]; 探测失败不阻断, 交给 ort 自行查找
fn degrade_on_probe_failure() {
    if let Err(e) = ensure() {
        log::debug!("ort 动态库探测未成功({e}), 交由 ort 自行查找");
    }
}

/// 桌面端探测 onnxruntime 库路径
#[cfg(not(target_os = "android"))]
fn find_library_path() -> Option<std::path::PathBuf> {
    use std::path::PathBuf;
    // 1. 显式环境变量
    if let Ok(p) = std::env::var("ORT_DYLIB_PATH") {
        if !p.is_empty() && Path::new(&p).exists() {
            return Some(PathBuf::from(p));
        }
    }
    // 2. 程序目录 / ./lib / ../lib
    let name = library_name();
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            dirs.push(dir.to_path_buf());
            dirs.push(dir.join("lib"));
        }
    }
    dirs.push(PathBuf::from("./lib"));
    dirs.push(PathBuf::from("./"));
    dirs.push(PathBuf::from("../lib"));
    dirs.iter().map(|d| d.join(name)).find(|p| p.exists())
}

#[cfg(not(target_os = "android"))]
fn library_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "onnxruntime.dll"
    } else if cfg!(target_os = "macos") {
        "libonnxruntime.dylib"
    } else {
        "libonnxruntime.so"
    }
}
