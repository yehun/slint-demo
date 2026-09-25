# 第十六篇 · Slint 语音克隆

给 Slint 做一个**本地零样本语音克隆**：丢进去一段 5~15 秒的人声，输入任意文本，
出来的音频就是那个人的声音。**无需参考文本转写**。全程离线，不联网、不上传任何音频。
底层是 LuxTTS（ZipVoice 蒸馏 flow-matching）的纯 Rust ONNX 推理。

```bash
# 1) 准备模型(约 120MB int8 / 700MB fp32, 仓库不分发)
./scripts/fetch-model.sh

# 2) 跑起来(界面里选参考音频 + 输入要合成的话, 无需参考文本)
cargo run -p article-16 --features desktop --release
```

不想开界面也可以只用 CLI 验证管线（免转写，只需参考音频 + 要合成的话）：

```bash
LUX_TTS_MODEL_DIR=$HOME/.local/share/slint-demo/models/lux-tts \
  cargo run --release -p slint-tts --example clone -- \
  ref.wav "要合成的文本" out.wav --model int8
```

## 实测（本机 Ryzen + int8 权重 + 8 线程，来自迁移验证跑批）

| 项 | 数值 |
|---|---|
| 模型加载 | 约 1.5 s（三个 ONNX：text_encoder / fm_decoder / vocos） |
| prompt 编码 | 参考音频 → 100 维 log-mel（超 15 s 截断） |
| 合成约 5 s 音频 | RTF 约 **1.1**（接近实时） |
| 输出 | 48 kHz 单声道 WAV |

RTF ≈ 1.1 已是"近实时"量级（生成 1 秒音频约花 1.1 秒）。参考音频越短越快
（attention 随帧数平方增长），日常用 5 秒左右的干净人声最划算。

## 管线：把一段声音"复制"出来（免转写）

```
参考音频 ──► 24kHz log-mel ──┐
                             ▼
                        text_encoder（时长预测 + 说话人条件）
                             ▼
        目标文本 ──► G2P(jieba+变调) ──► fm_decoder（8 步流匹配 ODE，CFG=3.0）
                             ▼
                    vocos 48k + 24k 双路声码器
                             ▼
              Linkwitz-Riley 12kHz 交叉合并 → 48kHz 波形
```

1. **参考音频 → prompt**：重采样到 24 kHz，算 100 维 log-mel；超过 15 s 截断
   （attention 随帧数平方增长，**不能裁静音**，裁了会改时长预测、语速当场崩）。
   音色条件只来自这段 mel，与文本无关，所以**无需参考文本**也能保住音色。
2. **目标文本 → 音素**：jieba 分词 + 拼音表（词组表命中直接取最终读音，否则逐字 + 三声/不/一变调），
   英文走 espeak-ng IPA（缺失时回退字母拼写）。这一步只作用于"要合成的话"。
3. **text_encoder**：把 prompt 的 mel 编码成条件，同时预测这段话要说多久。
4. **fm_decoder**：从高斯噪声出发，8 步锚点 ODE（流匹配），CFG 3.0 把结果拉向参考说话人。
5. **vocos**：48 kHz 路给高频、24 kHz 路给低频，Linkwitz-Riley 交叉合并，最后裁掉开头静音。

> **为什么能免转写？** 参考音频已经提供了说话人音色（mel 条件），而时长预测只需知道
> "参考音频大概说了多长"（按有效语音时长估算字数，填一串中性空格 token 当长度线索），
> 不依赖任何文本转写。实测在放宽语速补偿阈值（默认 0.95）后，免转写与"给正确转写"
> 的内容识别率一致，音色不受影响。

## 目录

```
crates/article-16/          # Slint UI（本文）
├── ui/app.slint            # 深色卡片式界面：模型 → 参考 → 文本 → 结果
├── src/lib.rs              # 入口 + 忙碌进度条
├── src/state.rs            # 跨线程状态（模型/波形/路径）
├── src/clone.rs            # 加载模型 / 选参考 / 合成 / 保存
└── src/player.rs           # rodio 播放内存里的 Vec<f32>
crates/slint-tts/           # 推理库（LuxTTS / ZipVoice 蒸馏 flow-matching: text_encoder + fm_decoder + vocos）
```

## 三条工程约定

1. **UI 线程不碰模型**。加载和合成都在 `std::thread` 里跑，结果经
   `slint::invoke_from_event_loop` 回写属性，界面始终不卡。
2. **波形留在内存**。合成结果是 `Vec<f32>`，rodio 的 `SamplesBuffer` 直接吃，
   保存时才编码 WAV。
3. **模型不进仓库**。几百 MB 的 ONNX 走 `scripts/fetch-model.sh` 单独下载，
   `LUX_TTS_MODEL_DIR` 指路，界面上也能手动选。

## Android

`make build-apk` 与其他篇一致，但注意：Android 下 `ort` 不能 `load-dynamic`
（bionic 的 `dlsym` 解析不了版本化符号），需要自备编译好的 `libonnxruntime.so`
并在 `slint-tts` 的 build 里链接。桌面端无此要求。
