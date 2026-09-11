package com.example.article10.clipboard;

import android.content.ClipData;
import android.content.ClipboardManager;
import android.content.Context;

/**
 * 剪贴板辅助类 — 供 JNI 调用
 *
 * Android 剪贴板操作必须在主线程执行,
 * 此类封装 ClipboardManager 的标准用法.
 *
 * 所有方法均为静态方法, 接收 Context 参数以获取系统服务.
 */
public class ClipboardHelper {

    /**
     * 写入文本到剪贴板 (静态方法, 供 JNI 调用)
     */
    public static void setTextStatic(Context context, String text) {
        ClipboardManager clipboard = getClipboardManager(context);
        ClipData clip = ClipData.newPlainText("slint-clipboard", text);
        clipboard.setPrimaryClip(clip);
    }

    /**
     * 从剪贴板读取文本 (静态方法, 供 JNI 调用)
     * 返回 null 表示剪贴板为空或非文本
     */
    public static String getTextStatic(Context context) {
        ClipboardManager clipboard = getClipboardManager(context);
        if (!clipboard.hasPrimaryClip()) {
            return null;
        }

        ClipData clip = clipboard.getPrimaryClip();
        if (clip == null || clip.getItemCount() == 0) {
            return null;
        }

        ClipData.Item item = clip.getItemAt(0);
        CharSequence text = item.getText();
        return text != null ? text.toString() : null;
    }

    /**
     * 获取 ClipboardManager 系统服务
     */
    private static ClipboardManager getClipboardManager(Context context) {
        return (ClipboardManager) context.getSystemService(Context.CLIPBOARD_SERVICE);
    }
}
