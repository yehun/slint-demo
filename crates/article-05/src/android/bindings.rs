// Android Java 类型绑定 - 使用 bind_java_type! 宏
// 声明式绑定 Java 类静态字段, 编译期类型安全, 运行时缓存 JClass/FieldID

use jni::bind_java_type;

// android.os.Build 类
bind_java_type! {
    pub Build => "android.os.Build",
    fields {
        #[allow(non_snake_case)]
        static MODEL { sig = JString, get = MODEL },
        #[allow(non_snake_case)]
        static MANUFACTURER { sig = JString, get = MANUFACTURER },
        #[allow(non_snake_case)]
        static BRAND { sig = JString, get = BRAND },
        #[allow(non_snake_case)]
        static DEVICE { sig = JString, get = DEVICE },
        #[allow(non_snake_case)]
        static HARDWARE { sig = JString, get = HARDWARE },
    },
}

// android.os.Build$VERSION 类 (内部类用 $ 分隔)
bind_java_type! {
    pub BuildVersion => "android.os.Build$VERSION",
    fields {
        #[allow(non_snake_case)]
        static RELEASE { sig = JString, get = RELEASE },
        #[allow(non_snake_case)]
        static SDK_INT { sig = jint, get = SDK_INT },
        #[allow(non_snake_case)]
        static CODENAME { sig = JString, get = CODENAME },
    },
}
