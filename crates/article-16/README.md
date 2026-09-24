# 第十六篇 · Slint 语音克隆

给 Slint 做一个**本地零样本语音克隆**：丢进去一段 5~15 秒的人声，输入任意文本，
出来的音频就是那个人的声音。全程离线，不联网、不上传任何音频。

```bash
# 1) 准备模型(约 200MB int8 / 800MB fp32, 仓库不分发)
./scripts/fetch-model.sh

# 2) 跑起来
cargo run -p article-16 --features desktop --release
```

不想开界面也可以只用 CLI 验证管线：

```bash
LUX_TTS_MODEL_DIR=$HOME/.local/share/slint-demo/models/lux-tts \
  cargo run --release -p slint-lux-tts --example clone -- \
  ref.wav "参考音频说了什么" "要合成的文本" out.wav --model int8
```

## 实测（本机 Ryzen + int8 权重 + 8 线程）

| 项 | 数值 |
|---|---|
| 模型加载 | 2.3 s（三个 ONNX：text_encoder / fm_decoder / vocos） |
| prompt 编码 | 0.36 s（14.2 s 参考音频 → 1332 帧 mel） |
| 合成 11.38 s 音频 | 24.9 s → **RTF 2.19** |
| 输出 | 48 kHz 单声道 WAV |

RTF 2.19 意思是"生成 1 秒音频要花 2.19 秒"，还做不到实时。参考音频越短越短越快
（attention 随帧数平方增长），日常用 5 秒左右的干净人声最划算。

## 管线：五步把一段声音"复制"出来

```
参考音频 ──► 24kHz log-mel ──┐
转写文本 ──► 音素 token ─────┤
                             ▼
                        text_encoder（时长预测 + 说话人条件）
                             ▼
        目标文本 ──► G2P ──► fm_decoder（8 步流匹配 ODE，CFG=3.0）
                             ▼
                    vocos 48k + 24k 双路声码器
                             ▼
              Linkwitz-Riley 12kHz 交叉合并 → 48kHz 波形
```

1. **参考音频 → prompt**：重采样到 24 kHz，算 100 维 log-mel；超过 15 s 截断
   （attention 随帧数平方增长，**不能裁静音**，裁了会改时长预测、语速当场崩）。
2. **转写文本 → 音素**：jieba 分词 + 拼音表（词组表命中直接取最终读音，否则逐字 + 三声/不/一变调），
   英文走 espeak-ng IPA。这就是为什么界面上"参考音频说了什么"是**必填**——
   没有它，音素和声学帧对不齐，克隆出来的口型是糊的。
3. **text_encoder**：把 prompt 的 mel + 音素编码成条件，同时预测这段话要说多久。
4. **fm_decoder**：从高斯噪声出发，8 步锚点 ODE（流匹配），CFG 3.0 把结果拉向参考说话人。
5. **vocos**：48 kHz 路给高频、24 kHz 路给低频，Linkwitz-Riley 交叉合并，最后裁掉开头静音。

## 目录

```
crates/article-16/          # Slint UI（本文）
├── ui/app.slint            # 深色卡片式界面：模型 → 参考 → 文本 → 结果
├── src/lib.rs              # 入口 + 忙碌进度条
├── src/state.rs            # 跨线程状态（模型/波形/路径）
├── src/clone.rs            # 加载模型 / 选参考 / 合成 / 保存
└── src/player.rs           # rodio 播放内存里的 Vec<f32>
crates/slint-lux-tts/       # 推理库（ONNX + G2P + mel + 声码器合并）
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
并在 `slint-lux-tts` 的 build 里链接。桌面端无此要求。
