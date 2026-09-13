package com.example.article11;

import android.app.NativeActivity;
import android.content.Intent;
import android.os.Bundle;

/**
 * 第十一幕: 文件选择与写入 — Android 入口.
 *
 * 必须继承 NativeActivity (Slint 要求),
 * 不能用普通 Activity — 否则 android_main 不被调用.
 *
 * 文件选择接入: SAF 的结果经 onActivityResult 回来,
 * 转发给 slint-file-picker 的 Java 类处理 (一行).
 */
public class MainActivity extends NativeActivity {
    static {
        System.loadLibrary("article_11");
    }

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
    }

    @Override
    protected void onActivityResult(int requestCode, int resultCode, Intent data) {
        super.onActivityResult(requestCode, resultCode, data);
        com.yehun.slintfs.SlintFilePicker.handle(requestCode, resultCode, data);
    }
}
