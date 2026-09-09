# EhViewer for Windows 11 —— 设置项与功能对位（parity map）

本文件记录 SXJ（Ehviewer_CN_SXJ，Android）与桌面客户端的设置/行为对位，重点是
**明显仅 Android 相关的项如何被适配或省略**及理由。持续更新（Phase 5 初稿）。

## 已在桌面端落地（Phase 5）

| 分组 | 设置 | 桌面实现 | 说明 |
| --- | --- | --- | --- |
| EH | 站点切换 | `site`（E/EX select） | 切换后经 `apply_settings` 更新 HTTP 客户端 Referer/Origin |
| EH | 身份 Cookie | `session_cookies`（ipb_member_id / ipb_pass_hash / igneous） | `cookie_save` 写盘时用 Windows DPAPI（`dpapi::protect`）加密；内存中为明文。沙箱受限令牌下 DPAPI 不可用则回退明文并记录 |
| EH | uconfig | `config: EhConfig` | 沿用 SXJ 序列化顺序生成 cookie |
| EH | 标签翻译 | `tag_translation_enabled` | 默认关；翻译库导入后续接入（见 §未实现） |
| 阅读 | 方向 / 缩放 / 起始位置 | `reading_direction` / `zoom_mode` / `reading_start_position` | 立即生效并持久化 |
| 下载 | 目录 / 线程数 / 原图 | `download_dir` / `download_threads` / `download_always_original` | 线程数 clamp 1–10；目录留空用 `图片/EhViewer` |
| 隐私 | 数据导出 / 导入 | `export_data` / `import_data` | 导出 settings + downloads + reading_progress 的 JSON |
| 高级 | 代理 / 超时 / 重试 / 缓存 / 自建 Host / DoH | `proxy_url` / `timeout_secs` / `max_retries` / `image_cache_size_mb` / `custom_host` / `doh_url` | `proxy_url` 与 `custom_host` 经 `apply_settings` 即时生效；`image_cache_size_mb` 经 `ehimg::set_cache_max_mb` 落地（最新优先淘汰）；DoH 解析器（`doh_url` + `use_builtin_hosts`）已后端生效（自实现 RFC8484 GET，无需 hickory） |

## 仅 Android 相关 —— 适配或省略（及理由）

| SXJ 设置/行为 | 桌面处理 | 理由 |
| --- | --- | --- |
| 屏幕旋转 / 横竖屏锁定 | **省略** | Windows 11 桌面无传感器旋转；窗口由用户自由调整 |
| 保持屏幕常亮 | **省略** | 桌面无息屏唤醒问题，由系统电源策略控制 |
| 时钟 / 电量 / 页码间隔浮层 | **省略** | 系统任务栏已提供时间/电量；页码沿用阅读器顶部显示 |
| 音量键翻页 | **适配** 为键盘 `PageUp` / `PageDown` / 方向键 | 桌面键盘（阅读器已有键盘翻页） |
| 蜂窝网络下载警告 | **省略** | 桌面一般为有线/Wi-Fi，无需流量提醒 |
| 媒体扫描（相册可见） | **省略** | 下载目录不参与相册/媒体库 |
| 图案锁 / 应用锁 | **省略** | Windows 采用系统账户/登录策略；相关内容受 DPAPI 保护 |
| WiFi 传输 / 局域网共享 | **省略** | 桌面版不提供跨设备传输 |
| 其它 Android 偏好（如通知栏进度） | **适配** 为 Tauri 事件 `download-progress` / `download-changed` | 由桌面 UI 订阅刷新 |

## 未实现 / 待办

- tag 翻译库（EhTagDatabase 格式）导入与显示（目标默认关闭）。
- `proxy_url`、`custom_host`、`image_cache_size_mb` 与 DoH 解析器（`doh_url` + `use_builtin_hosts`，自实现 RFC8484 GET，无需 hickory）均已后端生效；`ehimg://` 缓存上限走 `image_cache_size_mb`（Phase 6 已办）。
- Cookie 在 dpapi 不可用环境下的强提示（当前回退明文）。
- 阅读器的自动翻页（SXJ 阅读间隔）可映射为键盘自动翻页，待接入。


