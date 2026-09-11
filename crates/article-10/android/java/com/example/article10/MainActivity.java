package com.example.article10;

import android.app.NativeActivity;
import android.os.Bundle;
import android.util.Log;

/**
 * 第十幕: Slint 剪贴板读写 — Android 入口.
 *
 * 必须继承 NativeActivity (Slint 要求),
 * 不能用普通 Activity — 否则 android_main 不被调用.
 */
public class MainActivity extends NativeActivity {
    static {
        System.loadLibrary("article_10");
    }

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
    }
}
