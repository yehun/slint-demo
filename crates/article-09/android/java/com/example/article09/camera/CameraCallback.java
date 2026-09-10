package com.example.article09.camera;

/**
 * JNI 回调: Java → Rust.
 *
 * 这两个 native 方法由 Rust 通过 jni_mangle 导出实现.
 * CameraService 采集到帧/状态后, 调用这两个方法通知 Rust 侧.
 */
public class CameraCallback {
    public static native void onCameraFrame(byte[] data, int width, int height);
    public static native void onCameraState(int state);
}
