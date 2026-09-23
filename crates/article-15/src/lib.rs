// 第十五篇, Slint 国际化(i18n)
//
// 核心知识点:
// 1. Slint @tr 宏标记 UI 文本; slint-build 的 with_bundled_translations 把 .mo bundle 进二进制
// 2. 运行时 slint::select_bundled_translation(locale) 热切换语言(已显示的文本立即刷新)
// 3. 桌面端(sys-locale)与 Android 端(JNI 调 Locale.getDefault)自动检测系统语言
// 4. Rust 侧字符串翻译: 内置 .mo 解析器 + include_bytes! 嵌入 .mo (与 Slint @tr 平行)
//
// 三端入口:
//   desktop_main  —— Windows / Linux / macOS
//   android_main  —— Android (cargo apk2)

slint::include_modules!();

mod i18n;

use slint::ComponentHandle;

// ---- 桌面端入口: 用 sys-locale 检测系统语言 ----
#[cfg(not(target_os = "android"))]
pub fn setup_i18n(main_window: &MainWindow) {
    i18n::init_translations();

    let system_locale = sys_locale::get_locale().unwrap_or_else(|| "en".to_string());
    main_window
        .global::<I18nModel>()
        .set_system_locale(system_locale.clone().into());

    let default_locale = if system_locale.to_lowercase().starts_with("zh") {
        "zh_CN"
    } else {
        "en"
    };
    apply_language(main_window, default_locale);
    // 首次进入显示欢迎语(切换时再改成 "Language switched")
    main_window
        .global::<I18nModel>()
        .set_rust_message(i18n::gettext("Welcome to Slint i18n").into());
}

// ---- Android 端入口: 用 JNI 检测系统语言 ----
#[cfg(target_os = "android")]
pub fn setup_i18n(main_window: &MainWindow, app: &slint::android::AndroidApp) {
    i18n::init_translations();

    let system_locale = detect_android_locale(app);
    main_window
        .global::<I18nModel>()
        .set_system_locale(system_locale.clone().into());

    let default_locale = if system_locale.to_lowercase().starts_with("zh") {
        "zh_CN"
    } else {
        "en"
    };
    apply_language(main_window, default_locale);
    main_window
        .global::<I18nModel>()
        .set_rust_message(i18n::gettext("Welcome to Slint i18n").into());
}

/// 切换语言: 切换 Slint bundled 翻译 + 更新当前语言属性
pub fn apply_language(main_window: &MainWindow, locale: &str) {
    if let Err(e) = i18n::select_translations(locale) {
        log::warn!("[LIB] select_bundled_translation failed: {e:?}");
    }
    main_window
        .global::<I18nModel>()
        .set_current_locale(locale.to_string().into());
}

/// UI 语言切换按钮回调
pub fn setup_ui_callbacks(main_window: &MainWindow) {
    let ww = main_window.as_weak();
    main_window
        .global::<I18nModel>()
        .on_change_language(move |locale| {
            if let Some(w) = ww.upgrade() {
                apply_language(&w, &locale);
                w.global::<I18nModel>()
                    .set_rust_message(i18n::gettext("Language switched").into());
            }
        });
}

#[cfg(feature = "desktop")]
pub fn desktop_main() {
    init_stderr_logger();
    log::info!("[LIB] desktop_main started");

    let main_window = MainWindow::new().expect("create window");
    setup_i18n(&main_window);
    setup_ui_callbacks(&main_window);

    main_window.show().expect("show window");
    slint::run_event_loop().expect("event loop");
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub fn android_main(app: slint::android::AndroidApp) {
    slint::android::init(app.clone()).expect("slint android init");

    let main_window = MainWindow::new().expect("create window");
    setup_i18n(&main_window, &app);
    setup_ui_callbacks(&main_window);

    main_window.show().expect("show window");
    slint::run_event_loop().expect("event loop");
}

/// Android: 通过 JNI 调 java.util.Locale.getDefault().toLanguageTag() 拿到系统语言。
/// 任何一步失败都回退 "en", 保证 demo 在 Android 上始终能跑(手动切换仍可用)。
#[cfg(target_os = "android")]
fn detect_android_locale(app: &slint::android::AndroidApp) -> String {
    use jni::objects::JString;
    use jni::{JavaVM, jni_sig, jni_str};

    let vm_ptr = app.vm_as_ptr() as *mut jni::sys::JavaVM;
    // jni 0.22: from_raw 不可失败(内部仅断言指针非空), 直接返回 JavaVM
    let vm = unsafe { JavaVM::from_raw(vm_ptr) };

    let result = vm.attach_current_thread(|env| -> Result<String, jni::errors::Error> {
        // jni 0.22 起: 类名/方法名用 jni_str! 生成 JNIStr, 方法签名用 jni_sig! 生成 MethodSignature
        let locale_cls = env.find_class(jni_str!("java/util/Locale"))?;
        let locale = env
            .call_static_method(
                locale_cls,
                jni_str!("getDefault"),
                jni_sig!(() -> java.util.Locale),
                &[],
            )?
            .l()?;
        let tag_obj = env
            .call_method(
                locale,
                jni_str!("toLanguageTag"),
                jni_sig!(() -> java.lang.String),
                &[],
            )?
            .l()?;
        // JObject -> JString -> String (try_to_string 是 0.22 的非弃用转换)
        let tag_jstr = env.cast_local::<JString>(tag_obj)?;
        Ok(tag_jstr.try_to_string(env)?)
    });

    match result {
        Ok(s) => s,
        Err(e) => {
            log::warn!("[LIB] locale detect failed: {e:?}");
            "en".into()
        }
    }
}

// 简单 stderr logger (无需额外依赖)。仅桌面端使用: Android 日志走 logcat, stderr 不可见。
#[cfg(feature = "desktop")]
fn init_stderr_logger() {
    use log::Log;
    struct StderrLogger;
    impl Log for StderrLogger {
        fn enabled(&self, _: &log::Metadata) -> bool { true }
        fn log(&self, record: &log::Record) {
            eprintln!("[{}] {}", record.level(), record.args());
        }
        fn flush(&self) {}
    }
    let _ = log::set_boxed_logger(Box::new(StderrLogger));
    log::set_max_level(log::LevelFilter::Info);
}
