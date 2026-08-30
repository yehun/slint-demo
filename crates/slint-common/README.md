# slint-common — 共享模板 (预留)

Slint 跨平台系列的**共享代码模板**, 当前预留未使用.

## 状态

- 早期版本提供 `slint_demo!` 宏自动生成 desktop/android/wasm 三端入口
- 现各 demo crate 已改为显式编写三端入口 (`src/main.rs` + `src/lib.rs`), 更清晰可控
- 保留此 crate 用于存放未来系列通用的共享工具 (如统一初始化、主题等)

## 结构

```
slint-common/
└── src/
    └── lib.rs              # slint_demo! 宏 (历史实现)
```

> 仓库总览: [slint-demo](../../README.md)