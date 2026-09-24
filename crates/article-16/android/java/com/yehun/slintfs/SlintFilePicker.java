package com.yehun.slintfs;

import android.app.Activity;
import android.content.Intent;

/**
 * slint-file-picker 的 Java 侧: 发起系统文件选择 (SAF), 把结果回传给 Rust.
 *
 * 接入两步 (demo 的 MainActivity):
 *   1. 本文件放进 demo 的 android/java/com/yehun/slintfs/ (保持包路径);
 *   2. MainActivity 里加一个转发:
 *
 *          @Override
 *          protected void onActivityResult(int requestCode, int resultCode, Intent data) {
 *              super.onActivityResult(requestCode, resultCode, data);
 *              SlintFilePicker.handle(requestCode, resultCode, data);
 *          }
 *
 * SAF 授权模型不需要任何存储权限.
 */
public class SlintFilePicker {
    public static final int REQUEST_CODE_PICK_FILE = 10001;
    public static final int REQUEST_CODE_SAVE_FILE = 10002;

    /** Java → Rust: 结果回传 (Rust 侧以 no_mangle 导出对应符号) */
    public static native void onPicked(int requestCode, int resultCode, String uri);

    /** 打开已有文件: ACTION_OPEN_DOCUMENT */
    public static void open(Activity activity, String[] mimeTypes) {
        Intent intent = new Intent(Intent.ACTION_OPEN_DOCUMENT);
        intent.addCategory(Intent.CATEGORY_OPENABLE);
        intent.putExtra(Intent.EXTRA_ALLOW_MULTIPLE, false);

        // SAF 要求 setType 给一个宽类型, 具体列表放 EXTRA_MIME_TYPES
        if (mimeTypes != null && mimeTypes.length > 0) {
            String first = mimeTypes[0];
            int slash = first.indexOf('/');
            String base = slash > 0 ? first.substring(0, slash + 1) + "*" : first;
            intent.setType(base);
            intent.putExtra(Intent.EXTRA_MIME_TYPES, mimeTypes);
        } else {
            intent.setType("*/*");
        }

        activity.startActivityForResult(intent, REQUEST_CODE_PICK_FILE);
    }

    /** 保存文件: ACTION_CREATE_DOCUMENT */
    public static void save(Activity activity, String mimeType, String fileName) {
        Intent intent = new Intent(Intent.ACTION_CREATE_DOCUMENT);
        intent.addCategory(Intent.CATEGORY_OPENABLE);
        intent.setType(mimeType == null || mimeType.isEmpty() ? "*/*" : mimeType);
        if (fileName != null && !fileName.isEmpty()) {
            intent.putExtra(Intent.EXTRA_TITLE, fileName);
        }
        activity.startActivityForResult(intent, REQUEST_CODE_SAVE_FILE);
    }

    /** demo 的 MainActivity.onActivityResult 里调用这一行 */
    public static void handle(int requestCode, int resultCode, Intent data) {
        if (requestCode != REQUEST_CODE_PICK_FILE && requestCode != REQUEST_CODE_SAVE_FILE) {
            return;
        }
        String uri = (data != null && data.getData() != null) ? data.getData().toString() : null;
        onPicked(requestCode, resultCode, uri);
    }
}
