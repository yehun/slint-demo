package com.example.article18;

import android.app.NativeActivity;
import android.os.Bundle;
import android.util.Log;

/**
 * 第十八幕: Slint 地图显示 — Android 入口.
 *
 * 必须继承 NativeActivity (Slint 要求), 不能用普通 Activity —— 否则 android_main
 * 不被调用。名字须与 Cargo.toml 里声明的
 * [[package.metadata.android.application.activity]] name = "com.example.article18.MainActivity" 一致。
 * 之前 android/ 目录缺失, 导致 APK 里根本没有这个类, 一启动就
 * ClassNotFoundException / 闪退; 补上本文件即修复。
 */
public class MainActivity extends NativeActivity {
    static {
        System.loadLibrary("article_18");
    }

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
    }
}
