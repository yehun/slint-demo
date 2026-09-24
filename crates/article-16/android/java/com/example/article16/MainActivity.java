package com.example.article16;

import android.app.NativeActivity;
import android.content.Intent;
import android.os.Bundle;
import android.util.Log;

/**
 * 第十六篇, Slint 语音克隆 — Android 入口
 *
 * 两件事:
 * 1) 加载 Rust 动态库 (article_16)
 * 2) 把 SAF 文件选择的结果转发给 slint-file-picker (选参考音频要用)
 *
 * 注意: Android 下 ort 不能 load-dynamic, 需要自备编译好的 libonnxruntime.so。
 */
public class MainActivity extends NativeActivity {
    private static final String TAG = "Article16";
    private static final int REQUEST_OPEN_DOCUMENT = 1001;

    static {
        System.loadLibrary("article_16");
    }

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
    }

    @Override
    protected void onActivityResult(int requestCode, int resultCode, Intent data) {
        super.onActivityResult(requestCode, resultCode, data);
        // 转发给文件选择器: 选中的音频以 content:// URI 形式回到 Rust 侧
        com.yehun.slintfs.SlintFilePicker.handle(requestCode, resultCode, data);
    }
}
