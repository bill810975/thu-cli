# thu-cli

[English](README.md) | [中文](README.zh-CN.md)

这是一个使用 Rust 编写的清华大学校园服务与 Learn 命令行客户端。

命令行工具实现了登录、SM2 密码加密、Cookie 持久化、SSO 漫游、
JSON/HTML/表格解析，以及针对服务操作的安全检查。运行时不依赖
Node.js、Python 或移动端应用。

本项目的实现与兼容性工作建立在这两个上游项目之上：

- [THU Info](https://github.com/thu-info-community/thu-info-app)，尤其是
  `thu-info-lib` 中的校园服务协议与行为；
- [learnX](https://github.com/robertying/learnX) 及其依赖
  `thu-learn-lib`，用于参考 Learn 协议覆盖和应用工作流。

以上引用只说明来源，不代表上游项目对本项目背书。

> **许可证边界：** 本仓库目前是 source-available（源码可见），不是
> OSI 定义的开源发行版。仓库中包含由 THU Info 派生、受 Business Source
> License 1.1 约束的代码，也包含 learnX 的修改版 MIT 条款。分发或在生产
> 环境使用前，请阅读 [docs/LICENSING.md](docs/LICENSING.md) 和
> [`licenses/`](licenses/) 中的上游许可文本。

## Quick Start
若希望让 agent 自动完成 clone、构建和安装，可以直接把
[prompts/install-thu-cli.md](prompts/install-thu-cli.md) 中的 prompt 交给
agent。

## 功能

- 直连校园网或通过 WebVPN 完成清华 Info 认证。
- Info 与 Learn 使用相互隔离的持久会话。
- 通过 `--trust-device` 建立接近浏览器的可信设备登录。
- 只读调用遇到失效会话时按需自动续期，不需要后台常驻进程。
- 隐藏密码输入、SM2 加密、多因素认证，以及可选的 Keychain 密码回退。
- 支持校园新闻、课程、作业、课表、日历、图书馆、体育、一卡通、网络、
  邮箱、发票、THOS、GitLab 等服务。
- 支持日常报销（`052`）、智能/境外票据（`051`）、境内差旅（`06`）、
  会议（`19`）和境外差旅（`1206`）草稿，包括各路由的报销事项、
  电子发票、附件、支付信息、国际差旅预算、人民币/外币明细、暂存、提交、
  删除和撤单。
- 支持 Learn 课程、通知、文件、作业、问卷和下载。
- 支持 JSON 输出、操作发现、显式变更确认和防覆盖下载。
- 提供可安装的 Codex skill 和可直接交给 agent 的安装 prompt。

只读的 Info 和 Learn 登录、会话续期已做过真实环境测试。写操作、账号具体
权限、验证码流程以及其余校园服务的覆盖范围与限制，以
[docs/COVERAGE.md](docs/COVERAGE.md) 为准。

日常报销客户端还使用真实账号完成了项目绑定和草稿暂存验证。新增的
智能/境外票据、境内差旅、会议和境外差旅契约来自当前 WSBX 前端，
并由离线夹具覆盖；仍需使用真实账号验证具体权限和可报销场景。客户端没有
提交真实审批单。

## 环境要求

- Git
- Rustup
- Rust 目标平台需要的 C 工具链
- 用于登录认证的清华大学账号
- 能访问校园网，或能够使用 WebVPN 访问受限服务

仓库通过 [`rust-toolchain.toml`](rust-toolchain.toml) 固定 Rust 版本。尚未
安装 Rustup 时可先从 <https://rustup.rs/> 安装。

## 克隆仓库

```sh
git clone https://github.com/bill810975/thu-cli.git
cd thu-cli
```

## 构建与测试

首次构建时，Rustup 会自动安装仓库固定的工具链。

```sh
cargo build --locked --release
cargo test --locked
```

推荐的发行前检查：

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
```

构建完成后的可执行文件位于 `target/release/thu-cli`。Windows 下对应
`target\release\thu-cli.exe`。

## 安装

Linux、macOS 和 Windows 的预编译包发布在
[Releases](https://github.com/bill810975/thu-cli/releases) 页面。下载对应
平台的压缩包后，可先用同页面的 `SHA256SUMS` 校验，再解压并将 `thu-cli`
（Windows 下为 `thu-cli.exe`）放入 `PATH`。

发布目标：

- macOS universal（同时支持 `x86_64` 与 Apple Silicon）
- Windows `x86_64`
- Linux `x86_64`，同一个二进制可直接用于 WSL

### 从源码安装

```sh
cargo install --path . --locked
thu-cli --version
```

### 安装本地 release 构建

```sh
mkdir -p "$HOME/.local/bin"
install -m 0755 target/release/thu-cli "$HOME/.local/bin/thu-cli"
```

确认 `$HOME/.local/bin` 已加入 `PATH`。

## 首次登录

Info 与 Learn 必须使用不同的会话文件。密码只在终端中读取，不能通过命令行
参数传入。

```sh
thu-cli --session "$HOME/.config/thu-cli/info.json" \
  login --username YOUR_STUDENT_ID --service info --trust-device

thu-cli --session "$HOME/.config/thu-cli/learn.json" \
  login --username YOUR_STUDENT_ID --service learn --trust-device
```

`--trust-device` 会请求持久化的可信浏览器会话。成功登录后，通常可以无密码
刷新会话：

```sh
thu-cli --session "$HOME/.config/thu-cli/info.json" \
  login --username YOUR_STUDENT_ID --service info \
  --refresh --trust-device --non-interactive

thu-cli --session "$HOME/.config/thu-cli/learn.json" \
  login --username YOUR_STUDENT_ID --service learn \
  --refresh --trust-device --non-interactive
```

CLI 会在 Cookie 配置旁保存不含密钥的续期元数据。只读调用发现会话失效时
会自动使用它刷新并重试一次；写请求一旦可能到达服务器，就不会被自动重放。
单次调用可用 `--no-auto-refresh` 关闭该恢复行为。

在 macOS 上，可以把密码作为 Keychain 回退，用于持久身份 Cookie 过期后的
重新登录：

```sh
thu-cli credential set --username YOUR_STUDENT_ID --confirm
thu-cli credential status --username YOUR_STUDENT_ID
```

Keychain 不是持久会话的必需项。如果明确选择 Keychain 回退，但通过 SSH
运行时钥匙串处于锁定状态，应在终端中先执行：

```sh
security unlock-keychain "$HOME/Library/Keychains/login.keychain-db"
```

不要为了初始化这个命令行工具而额外打开图形终端。

会话生命周期、MFA、WebVPN 与凭据边界的更多说明见
[docs/auth.md](docs/auth.md)。

## 查看和运行操作

以命令行工具显示的服务目录为准：

```sh
thu-cli services
thu-cli services thu
thu-cli services learn
thu-cli services app
thu-cli describe learn homework
```

示例：

```sh
thu-cli --compact --session "$HOME/.config/thu-cli/info.json" \
  thu news.list --no-roam --args '{"page":1,"length":10}'

thu-cli --compact --session "$HOME/.config/thu-cli/info.json" \
  thu news.search --no-roam \
  --args '{"keyword":"任免","page":1,"exact":false}'

thu-cli --compact --session "$HOME/.config/thu-cli/learn.json" \
  learn semesters
```

日常报销的只读入口：

```sh
thu-cli --compact --session "$HOME/.config/thu-cli/info.json" \
  thu wsbx.business-types

thu-cli --compact --session "$HOME/.config/thu-cli/info.json" \
  thu wsbx.project.list --args '{"page_size":10}'

thu-cli --compact --session "$HOME/.config/thu-cli/info.json" \
  thu wsbx.draft.list --args '{"status":"ALL-2"}'
```

报销 SSO ticket 和会轮换的 API token 都通过已登录的 Info 会话按需获取。
token 保存在权限为 `600` 的私有 sidecar 中，不会输出。完整的草稿、发票、
附件、暂存和显式提交流程见 [docs/wsbx.md](docs/wsbx.md)。

## 安全模型

- 只读操作不需要 `--confirm`。
- 会修改服务端状态的操作必须显式传入 `--confirm`。
- `--dry-run` 只校验操作元数据，不发送网络请求。
- 修改请求超时并不代表没有成功；重试前应先检查服务端状态。
- 下载与导出不会覆盖已有文件。
- Cookie 文件与密码文件必须使用 `600` 权限。
- 不要在仓库、日志或 issue 中提交会话、Cookie、密码、MFA 验证码、
  token 或真实账号数据。

不要使用这个工具绕过验证码、MFA、Passkey、访问控制或学校政策。

## Codex skill

仓库包含两个可安装的 Codex skill：

```sh
mkdir -p "$HOME/.codex/skills"
cp -R skills/thu-cli "$HOME/.codex/skills/"
cp -R skills/thu-reimbursement "$HOME/.codex/skills/"
```

该 skill 介绍了操作发现、Info 与 Learn 工作流、会话续期、变更安全与密钥
处理。报销 skill 会区分国内票据与境外 `INVOICE`/`RECEIPT`，并给出材料、
审批、换汇和 CLI 操作路径。若希望让 agent 自动完成 clone、构建和安装，
可以直接把
[prompts/install-thu-cli.md](prompts/install-thu-cli.md) 中的 prompt 交给
agent。

## 仓库结构

```text
src/                 Rust 实现
tests/               进程级 CLI 测试
docs/                协议、覆盖范围、测试与许可说明
licenses/            上游项目与依赖许可
examples/            示例输入
skills/thu-cli/      可复用 Codex skill
skills/thu-reimbursement/  国内外票据报销流程 skill
prompts/             agent 安装 prompt
```

## 开发

```sh
cargo fmt --all
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
```

提交前请确认没有加入密钥、本机路径、真实账号数据或不兼容的第三方代码。
参见 [CONTRIBUTING.md](CONTRIBUTING.md) 与 [SECURITY.md](SECURITY.md)。

## 许可证

仓库中原创的独立 Rust glue code 按
[`licenses/original-glue-MIT.txt`](licenses/original-glue-MIT.txt) 中的 MIT
文本提供，但不会改变派生部分已有的权利与限制。

- THU Info 派生部分仍受 Business Source License 1.1 约束，Additional Use
  Grant 为 `None`，直至上游 Change Date。
- learnX 的通知包含额外的机构使用限制。
- thu-learn-lib 使用 MIT。
- Rust 依赖与 Rust 标准库各自保留其许可证，全文位于
  [`licenses/`](licenses/)。

根目录 [LICENSE](LICENSE) 是组合发行说明，不代表整个仓库可以按 MIT
重新许可。本仓库不构成法律意见。
