# Live2D 与 3D 路线

对应 [Issue 3](https://github.com/yokyora/helloworld/issues/3)。核对对象是当前工作区 `files/live2d/arti/`（`26b7ff3`）。本工作区没有 `docs/ARCHITECTURE.md`，窗口仍是 `src-tauri/tauri.conf.json` 里的 800×600 不透明窗口。

3D 的文件格式和运行库不在本文选定。

## 结论

- Live2D 模型本体可以交给 Cubism 4 或 5 的 Core 加载。它不是一套能直接当桌宠动作组用的完整包，目录里也没有分发授权。
- 显示用文件约 8.3 MB。另外 2222 条语音约 112.8 MB，和模型显示无关。
- WebView2 与 Edge 同一类引擎，官方 Web SDK 覆盖 Edge。不确定的是透明窗口和 WebGL 透明合成，要先做试验，再写角色功能。
- 仓库没有 3D 文件。3D 在格式、运行库、素材、许可四项确认前不实现。

## Live2D

### 现有素材

`files/live2d/arti/` 共 2249 个文件，121.1 MB。

| 内容 | 数量 | 体积 | 说明 |
| --- | --- | --- | --- |
| `Moc_0.moc3` | 1 | 0.68 MB | 文件头 `MOC3`，版本号 4 |
| 贴图 | 2 | 7.45 MB | `Textures_0_0.png` 4096×4096；`Textures_1_0.png` 2048×2048 |
| `model0.json`、物理、动作、表情 | 24 | 0.1 MB | 见下 |
| 语音 mp3 | 2222 | 112.8 MB | 只有音频 |

`model0.json` 是 Cubism model3 设定（`Version: 3`）。除语音外，JSON 引用的 26 个文件都在。

能被官方设定解析器读到的部分：

- 网格：`Moc_0.moc3`
- 贴图两张
- 物理：`Physics_0.json`，`Version: 3`
- 动作：`hair` 3 条、`face` 4 条，都是 motion3（`Version: 3`）
- 表情：16 个，`Type: Live2D Expression`

不能原样当作桌宠动作包：

- 没有 `Idle` 组，也没有官方 `Groups`。眨眼、口型写在扩展字段 `Controllers` 里，官方 Framework 不执行。
- `face` 第一条指向 `Motions_hair_0_File_0.json`，和头发动作重复。
- `voice` 有 2222 条，每条只有 `Sound`，没有动作文件。
- `Controllers`、`Options`、`PhysicsV2`、HitArea 上的 `Motion` 是扩展字段。官方解析器会忽略，不影响网格和贴图。
- 文件名是 `model0.json`，不是 `.model3.json`。把 JSON 内容交给设定解析器即可，不依赖扩展名。
- 没有 `cdi3`、pose、userdata。运行时显示不需要这些文件。

未压缩上传时，两张贴图约占 80 MB 显存（4096×4096 与 2048×2048，按 RGBA8 计算）。启动时不要加载 2222 条语音。

### 运行库

moc 版本 4 只能用 Cubism Core 4 或 5。Cubism 2 的 `live2d.min.js` 和 Cubism 3 Core 读不了这个文件。

桌面窗口走 WebView2，对应官方 **Cubism SDK for Web**：

- Core：专有库，不可改。当前包是 JavaScript，较新版本另带 WASM。
- Framework：随 SDK 以源码提供，许可证是 Live2D Open Software License，不是 MIT。
- 封装若内部仍调用这份 Core，许可证不因封装变成 MIT。本文不选定哪一个封装。

Core 随应用分发，不单独再发布成通用库。下载 SDK 时就要同意 Proprietary Software License 和 Open Software License。

### WebView2

官方 Web SDK 列出 Microsoft Edge，使用 WebGL。WebView2 是同一类 Chromium 内核，WebGL 和 WASM 可用。当前 `csp` 为 `null`。以后若收紧 CSP，WASM 需要相应放行。

`fetch` 不能直接读 `file://`。模型目录要由开发服务器、打包资源或 Tauri 资源协议提供，JSON 里的相对路径才有效。

官方没有保证 Tauri 透明窗口。WebGL 默认帧缓冲不透明，透明桌宠要单独验证：窗口透明、背景 alpha 为 0、WebGL `alpha`、预乘 alpha、点击与拖动的命中、DPI、卸载时释放 WebGL 资源。这一步不过，Live2D 和 3D 都不进入角色功能。

### 许可证

两套许可彼此独立。

**SDK。** 依据 Live2D 帮助文档（sdk_001、sdk_007）和扩展性应用说明：

- 开发和验证阶段不需要出版许诺合同。
- 发布普通应用时，最近年销售额低于 1000 万日元的个人和小规模事业者，免 SDK Release License（出版许诺合同）及费用。
- 年销售额达到 1000 万日元的中规模及以上事业者，发布前需要签约付费。
- 扩展性应用是例外：允许用户导入或组合不定量的模型（例如形象系统、模型查看器）。发布前必须单独审查和签约，个人和小规模事业者也不豁免。审查通常还要求有效的收费方式。
- 本任务只内置指定模型，不提供任意模型导入。

**素材。** `files/live2d/arti/` 没有许可文件。已有文件不等于可以打包分发。渲染试验使用 Cubism SDK 附带的官方示例模型，并遵守示例数据条款。Arti 只在本地核对格式，不放进安装包，也不引入新的来源不明的模型。

## 3D

仓库中没有 glTF、VRM、FBX、PMX 或其他三维模型。没有可测量的体积，也没有可核对的素材许可。

格式和运行库保持未定。以后只能在同时满足下面四条的方案里选：

1. 在 WebView2 里绘制，并能输出带 alpha 的画面，供透明窗口合成。
2. 能接上统一的加载、状态、卸载接口，卸载时释放 GPU 资源。
3. 运行库许可允许放进 Windows 安装包再分发。
4. 模型本身另有明确授权，不把运行库许可当成模型许可。

换到 WebView 之外的原生图形栈会改变窗口合成，不在本阶段考虑。

在四项确认之前，3D 的阻塞项是：无格式、无运行库、无素材、无许可。

## 实施顺序

1. 透明窗口与 WebGL 生命周期试验。用最小画面验证透明、命中、DPI、失败回退和卸载。不接具体角色格式。
2. 渲染接口：`load`、`setState`、`unload`。与聊天、存储、窗口控制分开。失败时回到现有角色，桌宠和面板仍可用。
3. Live2D 适配。用官方示例模型接通加载、待机、一套表情和一套点击动作。
4. 用 Arti 在本地核对本文列出的差异。不打包语音，不放进安装包。
5. 把待机、点击、休息、叫醒、勿扰接到 `setState`。拖动和快捷入口保持原样。休息和勿扰限制动画与互动。
6. 3D。等格式、运行库、素材、许可确认后再做。不在接口里写死格式。

第 5 步只依赖第 2 步的接口，可与第 3 步的后半段并行。第 6 步不进入当前实现。

## 拆分

Issue 3 拆成独立提交，避免一个变更同时绑定两条路线。

| 项 | 内容 | 依赖 | 状态 |
| --- | --- | --- | --- |
| 3.1 | 本文 | 无 | 记录 |
| 3.2 | 透明窗口与 WebGL 生命周期 | 无 | 可做 |
| 3.3 | 渲染接口与失败回退 | 3.2 | 可做 |
| 3.4 | Live2D，官方示例模型 | 3.3 | 可做 |
| 3.5 | Arti 本地差异核对 | 3.4 | 只本地，不分发 |
| 3.6 | 待机、点击、休息、叫醒、勿扰 | 3.3 | 可做 |
| 3.7 | 3D | 格式、运行库、素材、许可 | 阻塞 |

3.4 实现时再决定 Framework 的接入方式。无论哪种接入，Core 都是 Cubism 4 或 5。3.7 不在这次选定格式或运行库。
