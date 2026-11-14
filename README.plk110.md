# Skyward · 一加 15（PLK110）移植与修复完整说明

> 当前版本：**上游 v0.3.1 + PLK110 build 22**。修复一加面板亮度初始化及调节，保留解密、触控、完整字体、触觉反馈、Wi-Fi 记忆/自动连接、静态启动首屏、GitHub 云更新及 fastbootd 时序修复。
>
> 下载见 [Releases](https://github.com/liyongshun114514/skyward/releases/latest)，本版说明见 [PLK110-V22.md](PLK110-V22.md)，来源仓库见 [ACKNOWLEDGEMENTS.md](ACKNOWLEDGEMENTS.md)。
>
> **以下章节为 v16 历史记录，不是当前使用说明。** 字体子集命令和“振动不可用”等旧结论不得照用；完整字体打包以 PLK110-V18.md / PLK110-V20.md 为准。

---

## 0. 一句话结论

v15 之前的所有版本都死在同一个地方：`bring_up()` 在把 `/dev/tcm_hbp0` 交给 `system:system` 时失败——**这台机器根本没有这个节点**——而这个错误被一路 `?` 上抛，导致后面「启动手机自有程序（触控 HAL）」和「启动密钥链（qseecomd / keymint / gatekeeper / weaver）」两段**从未执行**。于是触控没反应、WLAN 的 supplicant 注册被拒、解密永远找不到 keymint——三个症状，一个根因。v16 恢复了「节点不存在就跳过」的容错，并把 `bring_up` 拆成互不连坐的阶段，同时修掉了两个此前被根因掩盖的独立缺陷（跨分区模块依赖、weaver 程序名匹配）。

---

## 1. 这是什么

**Skyward** 是一个用 Rust 从零编写的 Android recovery（上游：`kirklin/skyward`，GPL-3.0-or-later）。它不基于 TWRP，自己实现了解密、安装、备份、文件管理、终端、双系统等功能，界面用自绘 UI（`skyward-ui`）。

**本仓库**是把 Skyward 移植到 **一加 15（OnePlus 15 / PLK110，高通 SM8850「骁龙 8 Elite Gen 5」）** 的工作副本，包含：

- 设备配置 `devices/plk110.toml`（上游没有这台机器）；
- 为这台机器补的通用能力：`skyward-programs`（从手机自己的分区里发现并启动它自己的程序）、`skyward-omapi`（安全元件服务）、`skyward-binder` 的守护端、`bootlog`（把启动日志写进 misc 分区）等；
- 本次修复（提交 `d586d5e`）。

---

## 2. 交付物

| 类型 | 路径 | 大小 | SHA-256 |
| --- | --- | --- | --- |
| **可刷镜像** | `workspace/skyward-plk110/out/skyward-plk110-v16.img` | 104 857 600 B | `9fcb2c5f8c8e9c0c474a57ecb6c00913e96584afc6d172c46521461aca321aa8` |
| **源码归档** | `workspace/skyward-plk110/skyward-plk110-src-v16.tar.gz` | 见文件 | 见同级 `.sha256` |
| **源码目录** | `shared/skyward-plk110/skyward/`（git 提交 `d586d5e`） | — | — |
| **修复报告** | `workspace/skyward-plk110/REPORT-v16.md` | 6 KB | — |
| **原厂 recovery（回滚用）** | `workspace/tmp/skyward-plk110-probe/stock_recovery.img` | 104 857 600 B | `520ddbe39169dd12e29420d19038fee3aff4f87fbafbf4f527a0c0cccd2a9a7c` |

镜像内部件（可独立核对）：

| 项 | 值 |
| --- | --- |
| boot header | v4（GKI），kernel 0 字节，ramdisk 57 614 416 字节 |
| AVB | 自建未签名 footer，`rollback_index = 1`（照抄原厂），release string `skyward-pack` |
| `system/bin/recovery`（= Skyward 主程序） | 8 793 144 B，SHA-256 `b5f5cc5fce41e5a53ad80e86748b426e92dd9f193f12db54af7fa0f5ff53ea2b` |
| `skyward/device.toml` | 与仓库 `devices/plk110.toml` 逐字节相同 |
| `skyward/system-init` | 1 044 720 B（静态链接，双系统首启程序） |
| 字体 | `skyward/MiSansVF.ttf` 4 200 888 B（子集）+ `KirkMono-Regular.ttf` 270 456 B |

---

## 3. 设备事实（真机实测，非推测）

| 项 | 值 |
| --- | --- |
| 型号 / 代号 | OnePlus 15 / `plk110`，平台代号 `canoe` |
| SoC | 高通 SM8850（骁龙 8 Elite Gen 5） |
| 屏幕 | 1272 × 2772，560 dpi，6.78"，圆角 176 px |
| 面板 | `qcom,mdss_dsi_panel_AD296_P_3_A0020_dsc_cmd` |
| 触控 | Synaptics **S3910**，走一加的 **HBP（host-based processing）** 层：内核只搬原始帧，坐标由用户态 HAL 生成 |
| 触控模块 | `oplus_hbp_core.ko`、`oplus_bsp_tp_hbp_syna_s3910.ko`（+`oplus_bsp_tp_syna_common`、`oplus_bsp_tp_notify`） |
| 触控节点 | 只有 `/dev/hbp0`、`/dev/hbp_core`（`crw-rw---- system system`）；**没有** `/dev/tcm_hbp0` |
| 触控 HAL | `/odm/bin/hw/vendor-oplus-hardware-touch-V2-hbp5-service`，服务名 `vendor.oplus.hardware.touch.IOplusTouch/default` |
| WLAN | 高通 **Peach v2**（PCIe），驱动 `qca_cld3_peach_v2` + `cnss2`，开机后 `wlan0` |
| WLAN 模块分布 | 40 个依赖在 `vendor_dlkm`，`rfkill.ko` 只在 `system_dlkm`（**跨分区依赖**） |
| 分区视图 | `/vendor/lib/modules → /vendor_dlkm/lib/modules`，`/system/lib/modules → /system_dlkm/lib/modules`（都是符号链接） |
| 驱动索取的固件名 | `wlan/WCNSS_qcom_cfg.ini`、`wlan/qca_cld/peach_v2/wlan_mac.bin`（从 `.ko` 字符串实取） |
| 配置文件实际位置 | `/mnt/vendor/persist/wlan/WCNSS_qcom_cfg.ini`（原厂脚本 `init.oplus.wifi.sh` 放的）；`wlan_mac.bin` 该机不存在，驱动自造 |
| 密钥链 | `qseecomd`（`vendor.sys.listeners.registered`）+ `onekeymint-service-qti`（`IKeyMintDevice/default`）+ `gatekeeper-rust-service-qti` + **NXP weaver**（`android.hardware.weaver-service.nxp`，用户 `vendor_nxp_weaver`=2911，组 `system drmrpc`）+ `secure_element-service.qti`（`ISecureElement/eSE1`）+ Skyward 自服的 `android.se.omapi.ISecureElementService/default` |
| 相关分区 | `recovery_a → /dev/block/sde27`（100 MB）；modem 挂 `/vendor/firmware_mnt`；persist 挂 `/mnt/vendor/persist` |

---

## 4. 它是怎么跑起来的（启动链）

```
Bootloader（AVB 校验 recovery_a，rollback_index=1）
  └─ 原厂 recovery 的 init（保持原样）
       └─ init.rc：system/bin/recovery → Skyward 主程序（替换原厂 recovery 可执行文件）
            ├─ 挂 super，映射并挂载 system / vendor / odm / product …
            ├─ keys::found()：读安装系统的 VINTF 清单与 init 脚本，算出「用这把手机自己的钥匙要做什么」
            ├─ bring_up()（后台线程，按阶段）
            │    1. ueventd 规则落实设备节点属主
            │    2. 采用安装系统的构建号（避免 TEE 认为版本回滚而拒绝密钥）
            │    3. logd（可选）
            │    4. 挂 modem → /vendor/firmware_mnt，并 bind 到 /firmware（固件加载器搜索路径）
            │    5. 挂 persist / vendor_dlkm / system_dlkm
            │    6. 加载模块（触控链；WLAN 链在打开 WLAN 时才加载）
            │    7. hand_over()：把内核层生成的节点交给手机 init 指定的账户
            │    8. 告诉服务目录「这台手机描述了哪些服务」（supplicant / 触控 / 密钥都要）
            │    9. 启动手机自有程序（触控 HAL 等）
            │   10. 启动密钥链（qseecomd → keymint → gatekeeper → 安全元件 → weaver → boot control）
            └─ UI 线程：面板 / 输入 / 页面（与 bring_up 并行，屏幕先亮）
```

**关键设计**：`bring_up` 的每个阶段原本串行且 `?` 直抛——任何一处失败，后面全部不执行。v16 把「分区挂载 / 模块 / 节点交还 / 手机程序 / 密钥 / shared-secret 协商」都改成「失败写日志、继续跑」，因为对用户来说「触控能用但 WLAN 不行」远好于「一崩全崩」。

---

## 5. 三个问题的根因与修法

### 5.1 证据：v15 的 misc 启动日志

Skyward 把启动日志写在 misc 分区第 8192 字节起的 48 KB 里（`crates/skyward/src/bootlog.rs`），重启不丢：

```sh
dd if=/dev/block/by-name/misc bs=4096 skip=2 count=12 2>/dev/null > misc.bin
# 在电脑上读取 misc.bin：只解释首个 NUL 之前的字节，不拼接残留尾部。
```

v15 实际输出（节选）：

```
modem is mounted on /vendor/firmware_mnt, shown at /firmware
devices not brought up: give /dev/tcm_hbp0 to system:system: No such file or directory (os error 2)
the panel reports 1 events
```

- `devices not brought up: …` —— `prepare()` 里 `bring_up()` 返回的 Err 被打印；
- `the panel reports 1 events` —— UI 面板在整段使用期只收到 1 个输入事件，等于没有触控。

### 5.2 根因链

1. `devices/plk110.toml` 的 `[[owners]]` 里写了三个节点：`/dev/hbp0`、`/dev/hbp_core`、`/dev/tcm_hbp0`。
2. `hand_over()` 对每个路径 `chown` 后 `?`——第三个节点不存在 → 直接返回 Err。
3. `bring_up()` 在 `hand_over` 之后还有 4 个阶段（发布服务描述、启动手机程序、启动密钥、协商 shared-secret）**一个都没跑**：
   - 触控 HAL 没启动 → HBP 帧没人转成触点 → 面板 1 个事件；
   - 服务目录没被告诉 supplicant 的服务 → `wpa_supplicant` 注册被拒 → WLAN 每次都同一个错；
   - 密钥链没启动 → 解密打开 keymint 服务失败 → 报错永远相同。
4. 为什么「早期版本能触控」：v6 附近的 `hand_over` 带 `ErrorKind::NotFound => continue`（`workspace/tmp/plkdbg/mine-crates-backup/skyward/src/main.rs`），重构进 `skyward-programs` 时这层容错丢失了。

### 5.3 五处修改（提交 `d586d5e`）

| # | 文件 | 改了什么 | 为什么 |
| --- | --- | --- | --- |
| 1 | `crates/skyward/src/main.rs` · `hand_over()` | 节点 `NotFound` 时 `continue`（其余错误照旧返回） | 与 init 的 `chown` 语义一致：内核层可能还没生成该节点；不能因为一个节点拖垮整条链 |
| 2 | `crates/skyward/src/main.rs` · `bring_up()` | 分区挂载、模块加载（逐模块）、`hand_over`、`services::start`、`keys::start`、`agree_on_the_notes_key` 全部改为「失败写 misc 日志后继续」；阶段结果统一走 `bootlog::line` | 一处失败不再连坐；下一轮排障能直接从 misc 看到停在哪 |
| 3 | `crates/skyward-modules/src/lib.rs` | 依赖图改为**合并所有分区的 `modules.dep`**；模块路径解析「自身所在目录优先，再按 sources 顺序」；新增回归测试 | `qca_cld3_peach_v2.ko` 的 41 个依赖里 `rfkill.ko` 只在 `system_dlkm`；旧实现只在 vendor 目录找，必然报 “listed in …/modules.dep but missing”，WLAN 在加载阶段就死 |
| 4 | `crates/skyward/src/wlan.rs` | `INTERFACE_WAIT` 20 s → 60 s；`place()` 源文件缺失时记录并跳过 | cnss2 要先拉起芯片处理器、下载固件、建立数据服务才出现 `wlan0`；一个可选 ini 不该挡住整个 WLAN |
| 5 | `crates/skyward-programs/src/secure.rs` | `WEAVERS` 增加 `(".nxp", Through::ElementService)` | 一加把 NXP weaver 命名为 `android.hardware.weaver-service.nxp`，原表只有 `.nxp-qti` / `weaver-service`，`ends_with` 匹配不到 → `weaver()` 直接 bail → 密钥链仍然起不来（这是解密的**第二个独立缺陷**） |
| 6 | `devices/plk110.toml` | `[wlan]` 增加 `firmware = [{ name = "wlan/WCNSS_qcom_cfg.ini", path = "/mnt/vendor/persist/wlan/WCNSS_qcom_cfg.ini" }]`；删除重复的 `android.hardware.boot-service.qti` 服务项 | 驱动确实向 firmware loader 索取这个路径；boot control 由密钥链自动发现启动，手工再起一份会造成同名服务注册失败 + 1 秒重启循环 |

> 注：表中编号 6 是配置项，代码改动实为 5 处。

### 5.4 修改后的启动日志（预期）

正常应在 misc 看到：

```
43 device nodes given the owner their phone describes
persist is mounted on /mnt/vendor/persist
vendor_dlkm is mounted on /vendor_dlkm
system_dlkm is mounted on /system_dlkm
loaded /vendor_dlkm/lib/modules/oplus_hbp_core.ko
loaded /vendor_dlkm/lib/modules/oplus_bsp_tp_hbp_syna_s3910.ko
3 of the phone's own files handed to the accounts its init gives them
the service directory is told of N of the phone's own services
not every one of the phone's own programs runs: …   ← 只有失败时出现
the phone's keys are not usable: …                  ← 只有失败时出现
```

并且 `bootlog::snapshot` 里的 `touch programs:` 应该能看到 `…/vendor-oplus-hardware-touch-V2-hbp5-service` 正在运行。

---

## 6. 源码结构

仓库根：`shared/skyward-plk110/skyward/`（Rust workspace，23 个 crate，170 个 `.rs`，约 7.9 万行）。

### 6.1 crate 一览

| crate | 作用 |
| --- | --- |
| `skyward` | 主程序：命令、启动流程（`main.rs`）、UI 循环（`ui.rs`）、触控输入（`input.rs`）、WLAN（`wlan.rs`）、解密（`decrypt.rs`）、密钥发现（`keys.rs`）、服务启动（`services.rs`）、启动日志（`bootlog.rs`） |
| `skyward-ui` | 自绘界面框架：绘制、动画、页面与控件 |
| `skyward-config` | `devices/*.toml` 的结构与校验、ramdisk 内路径常量 |
| `skyward-programs` | **关键模块**：把安装系统分区里的 VINTF 清单 / init 脚本 / passwd 读成「手机有哪些程序、怎么起、注册什么服务」（`phone.rs`、`script.rs`、`secure.rs`、`naming.rs`、`description.rs`） |
| `skyward-omapi` | 安全元件服务（`android.se.omapi.ISecureElementService/default`），weaver 通过它访问 eSE |
| `skyward-binder` | Binder 协议：驱动、Parcel、服务目录查询与注册、守护端 |
| `skyward-crypto` | FBE 解密：keymint 客户端、`data.rs` 打开/解锁、shared-secret 协商 |
| `skyward-vab` | Virtual A/B：super 解析、槽位、快照 |
| `skyward-block` | 分区表 / fstab / mount / dm / by-name |
| `skyward-bootimg` | boot image v3/v4 读写、newc(cpio) 归档 |
| `skyward-pack` | 打包器：取原厂 recovery、替换 ramdisk、AVB footer、`list`/`cat`/`repack`/`described` 工具 |
| `skyward-modules` | 模块加载：`modules.dep` 依赖图、`finit_module`、libmodprobe 兼容（本次修改点） |
| `skyward-sys` | 系统调用封装：kmsg、property、uevent 套接字、ueventd 规则与账号、USB 配置、休眠 |
| `skyward-info` | 机型 / 屏幕 / CPU / 电池等信息 |
| `skyward-install` | 安装四种包（含 sideload） |
| `skyward-backup` | 备份 / 恢复（EFS、应用、IMEI） |
| `skyward-files` | 文件操作（复制 / 移动 / 改名 / 删除） |
| `skyward-mtp` | MTP（电脑上看到内部存储） |
| `skyward-net` | 网络（DHCP、路由、连通性探测） |
| `skyward-lp` | 逻辑分区 / super 工具 |
| `skyward-systems` | 双系统：第二系统的根目录与启动 |
| `skyward-system-init` | 双系统首启程序（静态链接） |
| `skyward-preview` | 在电脑上把界面渲染成 PNG（无需手机） |

### 6.2 本次改动涉及的关键位置

| 位置 | 说明 |
| --- | --- |
| `crates/skyward/src/main.rs:521` `bring_up()` | 启动主流程，本次改为分阶段容错 |
| `crates/skyward/src/main.rs:608` `hand_over()` | 设备节点交还，本次恢复 `NotFound` 容错 |
| `crates/skyward/src/main.rs:692` `mount_firmware()` | modem → `/vendor/firmware_mnt`，并 bind 到 `/firmware` |
| `crates/skyward-modules/src/lib.rs:29` `load()` | 模块加载，本次改为跨分区解析 |
| `crates/skyward-modules/src/lib.rs:59` `parse_deps()` | 合并多个分区的 `modules.dep` |
| `crates/skyward/src/wlan.rs:43` `INTERFACE_WAIT` | 20 s → 60 s |
| `crates/skyward/src/wlan.rs:694` `place()` | 固件放置，缺失即跳过 |
| `crates/skyward-programs/src/secure.rs:63` `WEAVERS` | weaver 程序名表，新增 `.nxp` |
| `devices/plk110.toml` | 设备配置（见第 11 章） |

---

## 7. 构建手册

### 7.1 环境

| 项 | 值 |
| --- | --- |
| 构建机 | Ubuntu 24.04 aarch64（容器内） |
| Rust | `rustc 1.100.0`（`rust-toolchain.toml`：stable + `aarch64-linux-android`） |
| NDK | `/opt/android-ndk/r29`（`Pkg.Revision = 29.0.14206865`） |
| 环境脚本 | `. /opt/toolchains/env.sh`（提供 `ANDROID_NDK_ROOT`、SDK、PATH） |
| 交叉编译器 | `$NDK/toolchains/llvm/prebuilt/linux-x86_64/bin/aarch64-linux-android30-clang` |

> 宿主是 aarch64，NDK 里 `prebuilt/linux-x86_64` 只是目录名沿用历史，工具本身是 arm64 原生可执行文件。

### 7.2 三条构建命令

```sh
cd shared/skyward-plk110/skyward
. /opt/toolchains/env.sh
B=/opt/android-ndk/r29/toolchains/llvm/prebuilt/linux-x86_64/bin
export CC_aarch64_linux_android=$B/aarch64-linux-android30-clang
export AR_aarch64_linux_android=$B/llvm-ar
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER=$B/aarch64-linux-android30-clang

# 1) 手机上的 recovery 服务
cargo build --release -p skyward --target aarch64-linux-android

# 2) 电脑上的打包器
cargo build --release -p skyward-pack

# 3) 双系统首启程序（静态链接；改动不多时可复用旧产物）
RUSTFLAGS='-C target-feature=+crt-static' \
  cargo build --release -p skyward-system-init \
  --target aarch64-linux-android --target-dir target/system-init
```

产物：

- `target/aarch64-linux-android/release/skyward`（≈ 8.8 MB）
- `target/release/skyward-pack`
- `target/system-init/aarch64-linux-android/release/system-init`（≈ 1.0 MB）

### 7.3 常见坑

- 不设 `CC_aarch64_linux_android` / `AR_aarch64_linux_android`：`ring` 的 build script 报 `ToolNotFound`。
- 环境脚本默认 linker 指向 `aarch64-linux-android24-clang`；本工程统一用 API 30，故显式覆盖（对应 `cargo ndk --platform 30`）。
- 本机没有安装 `cargo-ndk`，上面用等价的环境变量方式代替，效果相同。

### 7.4 测试

```sh
cargo test --workspace                       # v16：841 passed / 0 failed
cargo clippy --workspace --all-targets -- -D warnings
```

`crates/skyward-programs/tests/described/` 存了 20 台机器的分区「描述快照」（VINTF、init 脚本、passwd 等），用于在电脑上验证「读分区 → 得出启动步骤」这条链路。本次新增 `skyward-modules` 的跨分区依赖测试 `follows_a_dependency_into_another_partition`。

---

## 8. 打包镜像

### 8.1 命令（本次实际使用）

```sh
./target/release/skyward-pack recovery \
  --stock /var/minis/workspace/tmp/skyward-plk110-probe/stock_recovery.img \
  --binary target/aarch64-linux-android/release/skyward \
  --system-init target/system-init/aarch64-linux-android/release/system-init \
  --config devices/plk110.toml \
  --font  /var/minis/workspace/tmp/plkdbg/MiSans-subset.ttf \
  --mono  resources/fonts/KirkMono-Regular.ttf \
  --mono-license resources/fonts/KirkMono-OFL.txt \
  --out /var/minis/workspace/skyward-plk110/out/skyward-plk110-v16.img
# → wrote … (0 bytes kernel, 57614416 bytes ramdisk, 104857600 bytes in all)
```

打包器做的事：解原厂 recovery 的 ramdisk → 把 `system/bin/recovery` 换成 Skyward → 塞入 `skyward/{system-init,device.toml,MiSansVF.ttf,KirkMono-Regular.ttf,KirkMono-OFL.txt}` → 改 `init.rc`（启动 Skyward、加 `skyward-mtp.rc`、改 `prop.default`）→ 重新压成 newc → 写 boot header v4 → 填满 100 MB 分区 → 末尾写**自建未签名 AVB footer**（`rollback_index` 照抄原厂 = 1）。

### 8.2 三条硬约束

1. **AVB `rollback_index` 必须等于原厂**（本机 = 1），否则 bootloader 拒绝，表现为「卡第一屏」。
2. **ramdisk 不能超过约 64 MB**（v15 之前实测）。当前 57.6 MB；若改用完整 `resources/fonts/MiSansVF.ttf`（20 MB，比子集多 15.8 MB）就会超限——所以必须用子集。
3. **`--stock` 必须是整分区镜像**（含原厂 AVB footer），不是解包后的 ramdisk。

### 8.3 字体子集

- 现成子集：`workspace/tmp/plkdbg/MiSans-subset.ttf`（4 200 888 B）
- 字符集：`workspace/tmp/plkdbg/chars.txt`（约 23 KB，UI 用到的全部字符）
- 重新生成：

```sh
pyftsubset resources/fonts/MiSansVF.ttf \
  --text-file=/var/minis/workspace/tmp/plkdbg/chars.txt \
  --output-file=/var/minis/workspace/tmp/plkdbg/MiSans-subset.ttf
```

> 改 UI 文案后要重新生成，否则新字缺字形。

### 8.4 校验镜像

```sh
./target/release/skyward-pack list out/skyward-plk110-v16.img | head   # header + 文件清单
./target/release/skyward-pack cat  out/skyward-plk110-v16.img system/bin/recovery | sha256sum
sha256sum out/skyward-plk110-v16.img
```

AVB footer 核对（大端）：

```python
import struct
d = open('skyward-plk110-v16.img','rb').read()
vbo, vbs = struct.unpack_from('>QQ', d[-64:], 20)
v = d[vbo:vbo+vbs]; off = 32 + 8*10
print(v[:4], struct.unpack_from('>Q', v, off)[0], v[off+16:off+64].split(b'\0')[0])
# b'AVB0' 1 b'skyward-pack'
```

---

## 9. 刷机与回滚

### 9.1 刷入 `recovery_a`（本次已执行，可重放）

设备上需要有 root（本机用 `android-root-cli`，即 su）。容器与设备共享网络命名空间，所以「容器起 HTTP 服务 → 设备 curl 127.0.0.1」这条路可用。

```sh
# 1) 容器内：起一个只监听本机的 HTTP 服务
cd /var/minis/workspace/skyward-plk110/out
python3 -m http.server 8765 --bind 127.0.0.1 &

# 2) 设备内：下载并核对
curl -s -o /data/local/tmp/v16.img http://127.0.0.1:8765/skyward-plk110-v16.img
sha256sum /data/local/tmp/v16.img          # 应为 9fcb2c5f…aca321aa8

# 3) 写入并回读校验
dd if=/data/local/tmp/v16.img of=/dev/block/by-name/recovery_a bs=4M
sync
sha256sum /dev/block/by-name/recovery_a    # 与镜像一致

# 4) 清理
rm -f /data/local/tmp/v16.img
pkill -f "http.server 8765"                # 注意：别让这行匹配到自己的 shell
```

本次记录：

| 阶段 | 结果 |
| --- | --- |
| 写入前 `recovery_a` | `160348cd…68f323b4`（v15） |
| 设备端下载校验 | 与源文件一致 ✅ |
| `dd` | 25 records，104 857 600 字节，`0.043 s` |
| 回读 | `9fcb2c5f…aca321aa8` ✅ |

### 9.2 进入 recovery

- 关机后按住 **音量减 + 电源** 直到出现一加 logo；
- 或系统内 `adb reboot recovery`（需已开 USB 调试）。

### 9.3 回滚原厂 recovery

```sh
curl -s -o /data/local/tmp/stock.img http://127.0.0.1:8765/stock_recovery.img   # 先把它放进 out/
dd if=/data/local/tmp/stock.img of=/dev/block/by-name/recovery_a bs=4M
sync
```

原厂镜像：`workspace/tmp/skyward-plk110-probe/stock_recovery.img`，SHA-256 `520ddbe3…cd2a9a7c`。

### 9.4 用 fastboot 刷（另一种方式）

```sh
fastboot flash recovery_a skyward-plk110-v16.img
```

Skyward 自己也提供「连接电脑」页，可以从电脑上直接写分区。

---

## 10. 设备配置 `devices/plk110.toml` 逐段说明

```toml
[device]
codename = "plk110"          # 决定 devices/plk110.toml、镜像名
brand = "OnePlus"
model = "OnePlus 15"
ab = true                    # A/B，recovery 分区在 recovery_a/_b
soc = { hans = "...", hant = "...", en = "..." }   # 关于本机页显示
os_version_prop = "ro.build.display.id"
```

```toml
[display]
width = 1272
height = 2772
density = 560
diagonal = 6.78
corner_radius = 176          # 原厂 WindowManager 报的圆角
```

```toml
[modules]
sources = ["vendor_boot", "vendor_dlkm", "vendor", "system_dlkm"]
load = ["msm_drm", "qcom-hv-haptics", "oplus_hbp_core", "oplus_bsp_tp_hbp_syna_s3910"]
```

- `sources` 决定四个模块目录（映射见 `skyward-modules::source_dir`）：
  `vendor_boot → /lib/modules`、`vendor_dlkm → /vendor_dlkm/lib/modules`、`vendor → /vendor/lib/modules`、`system_dlkm → /system_dlkm/(flatten/)lib/modules`。
- `load` 里的每个名字都会连带 `modules.dep` 的依赖一起加载；依赖可以跨分区（v16 修复点）。
- 触控链实际加载：`oplus_hbp_core`（+`oplus_bsp_tp_notify` 等）+ `oplus_bsp_tp_hbp_syna_s3910`（+`oplus_bsp_tp_syna_common`）。

```toml
[[owners]]
user = "system"
group = "system"
mode = 0o660
paths = ["/dev/hbp0", "/dev/hbp_core", "/dev/tcm_hbp0"]
```

- 模拟原厂 init 的 `chown system system /dev/hbp0` 等；
- `/dev/tcm_hbp0` 在这台机器上**不存在**，v16 起「不存在就跳过」，写在这里是为了保持与原厂语义一致、将来若出现也能接管。

```toml
[firmware]
partition = "modem"
```

- 把 modem 分区挂到 `/vendor/firmware_mnt`，并 bind 到 `/firmware`（`FIRMWARE_SHOWN`）。
- 内核固件加载器（recovery 里由原厂 ueventd 代理）会搜索 `/etc/firmware/`、`/vendor/firmware/`、`/firmware/image/`、`/vendor/firmware_mnt/image/`；WLAN 的 `peach/bdwlan.*`、`tmel_peach_*.elf` 就在 `/firmware/image/` 下。
- 小提示：文件里的注释写的是「也显示在 /vendor/firmware」，实际代码 bind 到 `/firmware`；以代码为准（注释是早期写法，未改以免与已刷镜像不一致）。

```toml
[partitions]
mounted = ["persist", "vendor_dlkm", "system_dlkm"]
```

- `persist` → `/mnt/vendor/persist`（密钥链的数据、WLAN 的 ini 都在这）；
- 两个 dlkm 分区挂上后模块才可见。

```toml
[[service]]
path = "/vendor/bin/sh"
args = ["/vendor/bin/init.kernel.post_boot.sh"]
```

- 让 CPU 从 performance governor 交回 WALT、允许 idle 掉电，否则空转发热。

```toml
[[service]]
path = "/odm/bin/hw/vendor-oplus-hardware-touch-V2-hbp5-service"
libraries = ["/system/lib64", "/vendor/lib64", "/odm/lib64"]
ready_service = "vendor.oplus.hardware.touch.IOplusTouch/default"
groups = ["root", "system"]
capabilities = ["SYS_NICE"]
```

- 触控 HAL：**没有它就没有触控**（面板只上报内核原始帧）；
- 只写 `groups` 不写 `user`，默认 root（与原厂 `init.touchDaemon.rc` 一致）。

```toml
[wlan]
modules = ["rfkill", "cnss2", "qca_cld3_peach_v2"]
writes = [{ path = "/sys/kernel/cnss/fs_ready", value = "1" }]
firmware = [{ name = "wlan/WCNSS_qcom_cfg.ini", path = "/mnt/vendor/persist/wlan/WCNSS_qcom_cfg.ini" }]
interface = "wlan0"
supplicant = "/vendor/bin/hw/wpa_supplicant"
libraries = ["/system/lib64", "/vendor/lib64", "/odm/lib64"]
service = "android.hardware.wifi.supplicant.ISupplicant/default"
probe = "http://conn1.oppomobile.com/generate_204"
```

- `rfkill` 显式排第一：它在 `system_dlkm`，v16 之前靠 `modules.dep` 找不到（现在能，但保留显式更稳）；
- `fs_ready=1` 是让 cnss2 拉起芯片处理器、下载固件、建 `wlan0`；
- `firmware` 里那条是驱动真实索取的名字（`wlan/WCNSS_qcom_cfg.ini`），源文件在原厂 `persist` 里；
- `libraries` 里 `odm/lib64` 在最后：wpa_supplicant 需要一加的库，但不能让 odm 的低层库顶掉 recovery 自己的。

```toml
[haptics]
resonance = "/sys/class/qcom-haptics/lra_frequency_hz"
resonance_in_hertz = true
strength = { lightest = 0.25, strongest = 0.75 }
```

- 这台机器的 `lra_frequency_hz` 当前读出来是 **0**，日志会看到 `aptics left alone: … says the motor swings at 0 Hz`，即振动暂时不工作（不影响三项功能）。

```toml
[backup]
efs = ["modemst1", "modemst2", "fsg", "fsc", "persist"]
data_skipped = ["vendor/tzstorage", "vendor/qtee", "vendor/secure_element", "vendor/tui", "vendor/shsusr", "vendor/fingerprint", "vendor/fingerprint_ori"]

[wipe]
logs = ["/data/vendor/tombstones", "/data/vendor/ramdump", …]
```

---

## 11. 日志与排障

### 11.1 读启动日志（最重要的一条命令）

```sh
dd if=/dev/block/by-name/misc bs=4096 skip=2 count=12 2>/dev/null > misc.bin
# 在电脑上读取 misc.bin：只解释首个 NUL 之前的字节，不拼接残留尾部。
```

- 位置：misc 分区偏移 8192，长度 48 KB（`bootlog.rs` 的 `START/LEN`），重启不丢；
- 每次写整段（末尾一个 NUL），所以读到的是**本次或上次启动的完整尾部**；
- 想看更早的记录：`count` 调大（最多 14 块 = 57344 字节已覆盖整个日志区）。

### 11.2 日志行含义

| 日志 | 含义 | 失败时怎么办 |
| --- | --- | --- |
| `== before the screen` + `cmdline/dri/backlight/dsi_display0/inputs/hbp/touch programs` | 屏幕起之前的快照 | 黑屏时看这里：`dri` 有没有 card0、`hbp` 两个节点在不在 |
| `N device nodes given the owner their phone describes` | ueventd 规则执行数 | 0 或很小 → `/vendor/etc/ueventd.rc` 没读到 |
| `X is mounted on Y` / `X is not mounted: …` | 分区挂载 | 失败不影响后续阶段（v16 起） |
| `loaded /…ko` / `{name} is not loaded: …` | 模块加载 | 失败会写原因（找不到、依赖缺、EEXIST 等都区分） |
| `N of the phone's own files handed to the accounts its init gives them` / `not every one …` | 设备节点属主 | 缺节点现在只跳过，不再中断 |
| `the service directory is told of N of the phone's own services` | 服务描述发布 | 缺这一步 → WLAN/触控注册会被拒 |
| `not every one of the phone's own programs runs: …` | 手机自有程序启动失败（含触控 HAL） | 带具体路径与原因 |
| `the phone's keys are not usable: …` | 密钥链失败 | 带具体步骤（qseecomd/keymint/gatekeeper/weaver/boot） |
| `the secure world did not agree on the key it signs its notes with: …` | shared-secret 协商失败 | 影响带锁屏密码的解密 |
| `the panel reports N events` | 整段使用期收到的输入事件数 | 只有 1 个 ≈ 触控没工作 |
| `no touch panel came up; the pages are worked with the keys` | 5 s 内没找到触摸设备 | 说明输入设备都没出现（驱动问题） |
| `aptics left alone: … 0 Hz` | 振动频率读到 0 | 振动不可用，与三项无关 |

### 11.3 症状 → 处置

| 症状 | 先看 | 处置 |
| --- | --- | --- |
| 触控完全没反应 | misc 的 `not every one of the phone's own programs runs`；`hbp:` 两个节点 | HAL 路径/库不对 → 改 `[[service]]`；节点缺失 → 模块没加载 |
| 触控只有零星点 | `the panel reports N events`、`inputs:` 里 `event7=touchpanel` | 面板固件版本不匹配（`/vendor/firmware/tp/24624/FW_S3910_*`） |
| WLAN 打不开 | `{module} is not loaded`、`the driver made no wlan0 in 60 seconds` | 模块链 / `fs_ready` / 固件路径；`dmesg | grep -i cnss` 看驱动日志 |
| WLAN 能开但连不上 | supplicant 是否起来（`/skyward/run/supplicant/sockets/wlan0`） | 库路径、`wifi` 用户（1010）权限 |
| 解密报错 | `the phone's keys are not usable` / `the secure world did not agree…` | 看具体步骤；weaver 名是否被识别（本次已加 `.nxp`） |
| 卡第一屏 | 用 `skyward-pack list` 看 header、看 AVB | `rollback_index` 必须 = 1；ramdisk < 64 MB |
| 黑屏但 adb 可用 | misc 的 `== before the screen` 段 | `msm_drm` 是否 loaded、`dri` 有没有 card0 |

### 11.4 设备侧取证命令（本次用的）

```sh
# 硬件/节点
ls -l /dev/hbp0 /dev/hbp_core
getprop vendor.sys.listeners.registered

# 模块与依赖
grep -E "cnss2|peach|rfkill" /vendor_dlkm/lib/modules/modules.dep
cat /sys/module/qca_cld3_peach_v2/parameters/fwpath

# 驱动索取的固件名（直接从 .ko 里抠字符串）
grep -a -o -E "[A-Za-z0-9_./-]*(bdwlan|WCNSS_qcom_cfg|wlan/qca_cld)[A-Za-z0-9_./-]*" \
  /vendor_dlkm/lib/modules/qca_cld3_peach_v2.ko | sort -u

# 服务声明与启动脚本
ls /odm/etc/vintf/manifest/ | grep -E "weaver|keymint|secure_element"
cat /odm/etc/init/android.hardware.weaver-service.nxp.rc
cat /vendor/etc/init/qseecomd.rc
```

---

## 12. 验证记录

### 12.1 已做

| 项 | 方法 | 结果 |
| --- | --- | --- |
| 设备事实 | 真机 root 读取（节点、VINTF、init 脚本、`modules.dep`、`.ko` 字符串） | 见第 3 章 |
| 根因定位 | v15 misc 日志 + 与 v6 源码对比 | `hand_over` 丢容错 |
| WLAN 依赖闭合 | 用真机两份 `modules.dep` + 两个目录清单按新算法复算 | `qca_cld3_peach_v2` 41 个依赖全部可解析（vendor 40 + system 1） |
| 触控依赖 | 同上 | `oplus_hbp_core` 4、`oplus_bsp_tp_hbp_syna_s3910` 5，全部可解析 |
| 编译 | `cargo build --release -p skyward --target aarch64-linux-android` | 通过（只剩 4 个 dead_code warning） |
| 单元测试 | `cargo test --workspace` | **841 passed / 0 failed** |
| 配置解析 | 打包器读取 `devices/plk110.toml` | 通过 |
| 镜像结构 | header v4、ramdisk 57 614 416、AVB 大端字段 | `rollback_index=1`、`skyward-pack` |
| 镜像内容 | `skyward-pack cat … system/bin/recovery | sha256sum` | 与编译产物逐字节一致 |
| 配置一致性 | `cat … skyward/device.toml` 与仓库 diff | 完全相同 |
| 刷写 | `dd` + 回读 | 与镜像一致 |

### 12.2 没做（需要真机）

- 触控是否恢复上报；
- `wlan0` 是否出现、能否扫描并连接；
- 解密能否用锁屏密码打开 `/data`；
- 双系统安装/切换；
- 振动（已知 `lra_frequency_hz=0`）。

---

## 13. 已知限制与 TODO

1. **三项仍未真机确认**——v16 只完成了「根因修复 + 静态验证 + 刷入」，行为要等你反馈。
2. **振动不可用**：`/sys/class/qcom-haptics/lra_frequency_hz` 读数为 0，`haptics.rs` 主动放弃；需要改成读别的节点（如 `lra_freq`）或直接用 `qcom-hv-haptics` 的 ff 接口实测。
3. **配置注释与代码不一致**：`[firmware]` 注释说「也显示在 `/vendor/firmware`」，实际是 `/firmware`。为保持与已刷镜像一致，本次未改注释。
4. **字体子集需随文案更新**（第 8.3 节）。
5. **WLAN MAC**：`wlan/qca_cld/peach_v2/wlan_mac.bin` 该机不存在，由驱动自造；如需固定 MAC，可把原厂 persist 里的文件补进 `firmware` 列表。
6. **clippy 未清零**：`skyward` 有 4 个 dead_code（`bootlog::programs/listing` 等）warning，不影响功能。
7. **目录体积**：仓库带 `target/` 约 7.9 GB；源码归档已排除。

---

## 14. 版本与变更记录

| 版本 | 内容 |
| --- | --- |
| v2–v9 | 早期探索：面板点亮、模块加载、UI 跑通 |
| v10–v13 | 设备配置成形（屏幕、触控链、密钥服务） |
| v14 | 引入 `bootlog`（把日志写进 misc） |
| v15 | 重构 `bring_up` 顺序、加入 `skyward-programs` 自动发现（**引入 hand_over 致命中断**） |
| **v16** | 修复：hand_over 容错、bring_up 分阶段容错、跨分区模块依赖、WLAN 等待与固件容错、weaver 名匹配、配置补 WLAN ini / 去重复 boot-service |

提交：`d586d5e plk110: bring the touch panel, the WLAN and the phone's keys up`（父提交 `ff1668f` 上游）。

---

## 15. 附录：本次涉及的镜像

`workspace/skyward-plk110/out/` 下保留历史镜像（均 104 857 600 B）：

```
skyward-plk110-v2.img … skyward-plk110-v15.img   # 历史归档
skyward-plk110-v16.img                            # 当前（已刷入 recovery_a）
stock-refooted.img                                # 早期实验产物
```

---

## 16. 许可

Skyward 本体：**GPL-3.0-or-later**（见 `LICENSE`）。本文档与设备配置随仓库同许可。
