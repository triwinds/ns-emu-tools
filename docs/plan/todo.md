## todos

- 为 citron 和 eden 下载安装添加 macOS 支持
- ~~windows ryujinx 版本检测也换成二进制，然后前端的 tips 描述也改一下~~ ✅ 已完成
  - ✅ Python 版本实现（utils/ryujinx_version_detector.py）
  - ✅ Rust 版本实现（src-tauri/src/services/ryujinx.rs）
  - ✅ 性能提升约 18 倍（~18秒 → <1秒）
  - 📝 前端 tips 描述待更新