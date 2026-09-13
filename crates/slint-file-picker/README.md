# slint-file-picker

跨平台文件选择器, 与 [`slint-fs`](../slint-fs/) 配套: 选完直接拿到 `PlatformPath`, 可立即读写.

| 平台 | 实现 | 限制 |
|---|---|---|
| desktop | `rfd` 原生对话框 (后台线程打开) | — |
| Android | SAF `ACTION_OPEN_DOCUMENT` / `ACTION_CREATE_DOCUMENT` | 无需存储权限 |
| WASM | 暂不支持 | — |

## 用法

```rust
use slint_file_picker::{pick_file, pick_file_to_save, FileFilter, PickResult};

// 打开
pick_file(
    vec![
        FileFilter::new("文本").extension("txt").extension("md").mime("text/plain"),
        FileFilter::new("图片").extension("png").extension("jpg").mime("image/png"),
    ],
    |result| match result {
        PickResult::Picked(path) => {
            let mut f = path.read_file().unwrap();
            let text = f.read_string().unwrap();
            // 更新 UI 请切回主线程:
            // slint::invoke_from_event_loop(move || { ... }).unwrap();
        }
        PickResult::Cancelled => {}
        PickResult::Error(e) => eprintln!("{e}"),
    },
);

// 保存
pick_file_to_save("output.txt", vec![], |result| { /* 同上 */ });
```

**回调在后台线程执行** (系统对话框是异步的), 更新 Slint UI 用
`slint::invoke_from_event_loop`.

## Android 接入 (两步)

1. 把 `android/java/com/yehun/slintfs/SlintFilePicker.java` 复制到 demo crate 的
   `android/java/com/yehun/slintfs/` (保持包路径, `java_sources` 不用改);
2. demo 的 `MainActivity` 里加一个转发:

   ```java
   @Override
   protected void onActivityResult(int requestCode, int resultCode, Intent data) {
       super.onActivityResult(requestCode, resultCode, data);
       com.yehun.slintfs.SlintFilePicker.handle(requestCode, resultCode, data);
   }
   ```

SAF 是系统级授权, manifest 无需任何存储权限.

## 机制 (对应系列文章第五幕 / 第九幕的 JNI 模式)

```
Rust pick_file()
  → JNI 调 SlintFilePicker.open(activity, mimeTypes)   [应用类: ClassLoader 加载 + Global 缓存]
  → startActivityForResult → 系统对话框
  → MainActivity.onActivityResult → SlintFilePicker.handle
  → static native onPicked                              [Rust no_mangle 导出, EnvUnowned]
  → mpsc channel → 等待线程 → 用户回调 (PlatformPath)
```
