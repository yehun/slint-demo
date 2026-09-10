// Desktop 入口 —— 调用 lib.rs 中的 desktop_main

fn main() {
    #[cfg(feature = "desktop")]
    article_09::desktop_main();
}
