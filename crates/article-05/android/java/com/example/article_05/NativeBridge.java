package com.example.article_05;

/**
 * JNI 反向调用桥接类
 *
 * Java 侧通过此类调用 Rust 导出的 native 方法。
 * Rust 侧使用 #[no_mangle] 导出函数:
 *   Java_com_example_article_05_NativeBridge_onEventFromJava(String name, String value)
 *
 * 数据流:
 *   Java → JNI → Rust → Slint UI 更新
 */
public class NativeBridge {

    static {
        System.loadLibrary("article_05");
    }

    /**
     * 从 Java 侧发送事件到 Rust
     *
     * @param name  事件名称 (如 "toast", "battery")
     * @param value 事件数据
     */
    public static native void onEventFromJava(String name, String value);

    /**
     * 便捷方法: 发送 Toast 事件
     */
    public static void sendToast(String message) {
        onEventFromJava("toast", message);
    }

    /**
     * 便捷方法: 发送电池信息
     */
    public static void sendBattery(int level) {
        onEventFromJava("battery", String.valueOf(level));
    }
}
