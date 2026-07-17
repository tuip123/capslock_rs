# CapsLock RS

CapsLock RS 是一个 Windows 平台的轻量键盘增强工具。它将 `CapsLock` 作为类似 `Ctrl`、`Alt`、`Shift` 的组合键，让用户通过 INI 配置自由定义按键、组合键和内置编辑动作。

本项目不准备复刻一个包含搜索、翻译、计算器和剪贴板工作流的综合效率工具。它只专注于一件事：把 `CapsLock` 变成稳定、可配置、易扩展的键盘修饰层。

项目目前处于早期开发阶段，里程碑 2 的通用键位与组合键模型已经可用；里程碑 3 和里程碑 4 均为部分完成。当前已有原生 GUI 设置页、基础映射编辑器、映射行级状态和保存前 INI 预览，真实窗口完整回归、发布准备等仍未完成。

## 当前功能

- 使用 `WH_KEYBOARD_LL` 全局低级键盘钩子处理 CapsLock 组合键。
- 使用 `SendInput` 模拟目标按键，并标记程序生成的事件以避免递归触发。
- 支持 CapsLock+ 风格的 `[Keys]`、`caps_*` 和 `keyFunc_*` INI 配置。
- 支持通用源组合键：`Ctrl`、`Alt`、`Shift`、`Win` 及左右版本可与常用键组合。
- 支持三类输出动作：内置函数 `keyFunc_*`、目标单键 `keyTarget_*`、目标组合键 `keyCombo_*`。
- 支持带次数参数的动作，例如 `keyFunc_moveUp(5)` 和 `keyFunc_selectUp(5)`。
- 支持按词移动、选择、Home/End/PageUp/PageDown 和按词删除。
- 支持单实例、后台运行、系统托盘、配置重载和打开日志。
- 支持原生 Win32 GUI 设置页，可从托盘打开，编辑基础设置和按键绑定后保存并自动重载。
- GUI 映射编辑器代码层面支持新增、更新、删除 `keyFunc_*`、`keyTarget_*`、`keyCombo_*` 三类映射，并在列表中显示正常、重复映射、非法输入组合、未知动作或配置错误状态；当前编辑行会显示保存前 INI 预览，真实窗口完整回归仍需继续。
- 支持 `system`、`zh-CN` 和 `en-US` 界面语言；托盘菜单、设置页文本、错误摘要和配置语言项已接入 i18n，底层配置解析明细仍以英文日志和详情为主。
- 支持当前用户开机启动。
- 可选以管理员身份运行，用于向管理员权限窗口发送按键。
- 支持 UTF-8、UTF-16 LE BOM 和 UTF-16 BE BOM 配置文件。
- GUI 使用独立设置模型并复用同一份 INI 解析、序列化和保存逻辑。

当前默认键位：

| 组合键 | 输出 |
| --- | --- |
| `CapsLock + H` | 左方向键 |
| `CapsLock + J` | 下方向键 |
| `CapsLock + K` | 上方向键 |
| `CapsLock + L` | 右方向键 |
| `CapsLock + Space` | Enter |
| `CapsLock + Q` | Backspace |
| `CapsLock + E` | Delete |
| `CapsLock + Z` | 上方向键 5 次 |
| `CapsLock + X` | 下方向键 5 次 |
| `CapsLock + Left Alt + A` | Ctrl + Left |
| `CapsLock + Left Alt + D` | Ctrl + Right |

## 快速开始

发布构建：

```powershell
cargo build --release
```

生成的程序位于：

```text
target\release\capslock_rs.exe
```

将 `capslock_rs.ini` 放在程序同目录并启动程序。当前仓库根目录的 `capslock_rs.ini` 是保守默认配置；`examples/capslock_rs.example.ini` 提供更完整的里程碑 2 示例。Release 构建使用 Windows 子系统，不显示控制台窗口；运行状态和常用操作可通过系统托盘管理。

开发与测试：

```powershell
cargo test
cargo build
```

本项目目前仅支持 Windows。

## 配置

当前使用的默认配置保持尽量保守，只启用基础移动、删除和按词移动。完整示例配置位于 `examples/capslock_rs.example.ini`，可以按需复制其中的映射到 `capslock_rs.ini`。

默认配置示例：

```ini
[general]
enabled = true
start_with_windows = false
run_as_admin = false
show_tray_icon = true
tap_capslock = toggle

[Keys]
caps_h=keyFunc_moveLeft
caps_j=keyFunc_moveDown
caps_k=keyFunc_moveUp
caps_l=keyFunc_moveRight
caps_space=keyFunc_enter
caps_q=keyFunc_backspace
caps_e=keyFunc_delete
caps_backspace=keyFunc_deleteLine
caps_z=keyFunc_moveUp(5)
caps_x=keyFunc_moveDown(5)
caps_lalt_a=keyFunc_moveWordLeft
caps_lalt_d=keyFunc_moveWordRight
; caps_r=keyTarget_f5
; caps_c=keyCombo_ctrl_c
; caps_lalt_shift_j=keyFunc_selectDown
; caps_u=keyFunc_selectUp(5)

[ui]
language = system
settings_backend = ini
settings_page = native
```

配置文件按以下顺序查找：

1. `CAPSLOCK_RS_CONFIG` 环境变量指定的路径。
2. 当前工作目录下的 `capslock_rs.ini`。
3. 程序同目录下的 `capslock_rs.ini`。
4. 开发环境中的项目根目录。

`[ui]` 中的 `language` 支持 `system`、`zh-CN` 和 `en-US`。`system` 会跟随 Windows 界面语言，中文系统使用简体中文，其他语言回退为英文。

`[Keys]` 的源组合键统一写成 `caps_<修饰键>_<按键>`。修饰键支持 `ctrl`、`lctrl`、`rctrl`、`alt`、`lalt`、`ralt`、`shift`、`lshift`、`rshift`、`win`、`lwin`、`rwin`，多个修饰键的顺序会自动标准化，例如 `caps_shift_lalt_j` 会保存为 `caps_lalt_shift_j`。

常用按键覆盖字母、数字、方向键、`space`、`tab`、`enter`、`escape`、`backspace`、`delete`、`insert`、`home`、`end`、`page_up`、`page_down`、`f1` 到 `f24`、常见标点、数字小键盘和常用媒体键。

完整示例文件覆盖基础移动、选择、目标单键、目标组合键、多修饰键源组合和媒体键：

```ini
; examples/capslock_rs.example.ini
caps_shift_h=keyFunc_selectLeft
caps_r=keyTarget_f5
caps_m=keyTarget_media_play_pause
caps_c=keyCombo_ctrl_c
caps_ctrl_shift_h=keyCombo_ctrl_shift_left
```

动作值分为三类：

| 配置值 | 作用 |
| --- | --- |
| `keyFunc_moveLeft(5)` | 执行内置函数，可带次数参数 |
| `keyTarget_f5` | 输出单个目标按键 |
| `keyCombo_ctrl_c` | 输出目标组合键 |

当前支持的内置动作：

| 配置值 | 作用 |
| --- | --- |
| `keyFunc_moveLeft(n)` / `keyFunc_moveRight(n)` | 左右移动 `n` 次 |
| `keyFunc_moveUp(n)` / `keyFunc_moveDown(n)` | 上下移动 `n` 次 |
| `keyFunc_moveWordLeft(n)` / `keyFunc_moveWordRight(n)` | 按词左右移动 `n` 次 |
| `keyFunc_selectLeft(n)` / `keyFunc_selectRight(n)` | 左右选择 `n` 次 |
| `keyFunc_selectUp(n)` / `keyFunc_selectDown(n)` | 上下选择 `n` 次 |
| `keyFunc_selectWordLeft(n)` / `keyFunc_selectWordRight(n)` | 按词左右选择 `n` 次 |
| `keyFunc_home(n)` / `keyFunc_end(n)` | Home 或 End `n` 次 |
| `keyFunc_pageUp(n)` / `keyFunc_pageDown(n)` | PageUp 或 PageDown `n` 次 |
| `keyFunc_enter(n)` | Enter `n` 次 |
| `keyFunc_backspace(n)` | Backspace `n` 次 |
| `keyFunc_delete(n)` | Delete `n` 次 |
| `keyFunc_deleteWord(n)` | Ctrl + Backspace `n` 次 |
| `keyFunc_forwardDeleteWord(n)` | Ctrl + Delete `n` 次 |
| `keyFunc_deleteLine(n)` | 删除当前整行 `n` 次 |

兼容说明：已有 INI 中的 `keyFunc_doNothing`、`none`、`off` 或 `disabled` 会被当作禁用该条映射并跳过，不作为 GUI 可新增的内置动作保存。

省略参数时，次数默认为 `1`。无法识别的单条映射会被跳过并写入日志，不会导致整份配置失效。

## GUI 设置页（部分完成）

可从托盘菜单打开设置页。当前 GUI 已支持基础开关、开机启动、管理员模式、托盘图标、CapsLock 单击行为、界面语言、配置路径和日志路径。

映射编辑器代码层面已支持新增、更新和删除 CapsLock 输入组合，并配置三类目标：内置函数、目标单键和目标组合键。映射列表会复用配置解析和校验结果显示行级状态；当前编辑行会预览保存前将写入的 INI 表达式，非法内容显示本地化错误摘要；保存时会先复用配置解析和校验规则，写入成功后自动重载运行时配置。

GUI 与手工编辑共用同一份 INI。已能识别的 `general`、`ui` 字段和三类 `[Keys]` 映射会在 GUI 保存后继续保留；但注释、未知键、原始顺序和原始格式目前不会保留。

当前仍未完成：

- 真实窗口下的中文和英文完整配置流程仍需继续手工回归。
- 行级状态已覆盖当前映射列表，但完整编辑期校验体验仍需继续回归。

## 管理员窗口

Windows 的权限隔离会阻止普通权限程序向管理员窗口注入输入。需要控制管理员权限程序时，可修改：

```ini
[general]
run_as_admin = true
```

程序会通过 UAC 以管理员身份重新启动。此选项默认关闭，因为多数日常程序并不需要提权。

## 项目方向

里程碑 4 当前状态：部分完成。后续开发应优先收尾 GUI 映射编辑器，不重新引入程序或脚本动作：

- 继续打磨编辑期校验反馈和真实窗口完整回归。
- 继续验证中文和英文界面下的完整配置流程。
- 许可证和来源声明已补齐；CI、发布包和兼容性验证仍属于后续里程碑。

完整路线见 [PLAN.md](./PLAN.md)。

## 非目标

以下功能不属于本项目的发展方向：

- 搜索栏或 QBar。
- 翻译服务。
- 计算器或计算草稿纸。
- 新闻、网络查询等在线服务。
- 多剪贴板工作流。
- 通用窗口管理套件。
- 按键映射触发启动程序、打开 URL 或执行脚本。
- 在主程序中嵌入可任意执行代码的脚本解释器。

如果一项功能不能直接服务于“CapsLock 组合键输入、按键模拟或配置管理”，原则上不加入核心程序。

## AI 辅助开发说明

本项目使用 AI 辅助开发。项目目标、功能取舍、交互习惯和验收结果由项目维护者决定；AI 参与代码分析、实现建议、代码编写、测试补充和文档整理。

AI 生成或修改的代码不被视为天然正确。进入发布版本前，仍需要人工确认设计、审查关键的 Windows API 与 `unsafe` 代码，并通过实际环境测试验证行为。

明确披露 AI 的参与，是为了让使用者能够据此判断项目的开发方式、成熟度和风险，而不是把 AI 作为质量保证或宣传标签。

## 致谢与来源

本项目的产品思路、使用习惯、INI 命名风格和部分动作名称均参考了 [CapsLock+](https://github.com/wo52616111/capslock-plus)。CapsLock+ 展示了将 CapsLock 发展成高效键盘功能层的完整可能性，本项目向原作者及贡献者致敬。

开发 CapsLock RS 不是因为 CapsLock+ 不好用，也不是为了否定原项目。相反，CapsLock+ 已经提供了成熟、高效且长期经受验证的使用体验。本项目改用 Rust 重新实现，主要原因是 AutoHotkey 编译程序及其全局键盘钩子、按键模拟等行为，在部分安全软件中容易触发启发式检测、风险提示或误报。Rust 实现的目标是减少对 AutoHotkey 运行时特征的依赖，并提供更明确的权限、输入事件和发布流程控制，但仍不能保证完全消除安全软件提示。

CapsLock RS 不是 CapsLock+ 的官方版本或官方继任项目。它是一个根据个人需求重新收窄范围、使用 Rust 独立实现的实验性项目。

CapsLock+ 使用 GPLv2 许可证。CapsLock RS 以 Apache-2.0 许可证发布源码，详见 [LICENSE](./LICENSE)。项目通过 [NOTICE](./NOTICE) 和 [THIRD_PARTY_NOTICES.md](./THIRD_PARTY_NOTICES.md) 记录源码归属、CapsLock+ 参考关系和第三方依赖声明。当前实现不包含 CapsLock+ 源码；如果后续引入受 GPLv2 约束的源代码，必须在发布前重新评估许可证兼容性和再分发条款。

## 开源状态

项目计划迁移到 GitHub 公开开发。当前源码使用 Apache-2.0 许可证，允许商用、修改和再分发，但再分发时需要保留版权、许可证和 NOTICE 声明。目前仍属于早期版本，配置格式、动作名称和内部结构可能发生调整。第一次正式公开发布前，仍需要完成 CI、发布构建说明、基础兼容性测试和安全边界检查。
