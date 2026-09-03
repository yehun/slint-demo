package com.example.article_05;

import android.app.NativeActivity;
import android.os.Handler;
import android.os.Looper;
import android.widget.Toast;

public class MainActivity extends NativeActivity {
    static {
        System.loadLibrary("article_05");
    }

    // 主线程 Handler, 用于将 Toast 操作 post 到主线程
    private final Handler mainHandler = new Handler(Looper.getMainLooper());

    /**
     * 显示 Toast (供 Rust 通过 JNI 调用)
     * 使用 Handler(Looper.getMainLooper()).post() 确保在主线程执行
     * 即使从 native 线程 (无 Looper) 调用也能正常工作
     */
    public void showToast(final String message) {
        mainHandler.post(new Runnable() {
            @Override
            public void run() {
                Toast.makeText(MainActivity.this, message, Toast.LENGTH_SHORT).show();
            }
        });
    }
}
