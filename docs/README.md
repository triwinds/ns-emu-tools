# NS Emu Tools 文档中心

欢迎来到 NS Emu Tools 的文档中心。这里包含了项目的完整技术文档。

## 文档目录

### 📚 核心文档

#### [架构文档 (architecture.md)](architecture.md)
项目的完整架构说明，包括:
- 整体架构设计
- 目录结构详解
- 核心模块说明
- 数据流分析
- 技术栈介绍
- 依赖管理

**适合人群**: 所有开发者、架构师、技术决策者

---

#### [API 参考文档 (api-reference.md)](api-reference.md)
后端 API 的完整参考手册，包括:
- API 设计原则
- 所有 API 接口详细说明
- 请求/响应格式
- 错误处理
- 使用示例
- 最佳实践

**适合人群**: 前端开发者、API 集成开发者

---

#### [开发指南 (development-guide.md)](development-guide.md)
开发者的实用指南，包括:
- 环境搭建
- 开发流程
- 代码规范
- 调试技巧
- 测试方法
- 常见问题解答

**适合人群**: 新加入的开发者、贡献者

---

#### [部署指南 (deployment-guide.md)](deployment-guide.md)
构建和部署的完整流程，包括:
- 本地构建
- CI/CD 配置
- 发布流程
- 更新机制
- 故障排查
- 性能优化

**适合人群**: DevOps 工程师、发布管理员

---

## 快速导航

### 我想...

#### 了解项目架构
→ 阅读 [架构文档](architecture.md)

#### 开始开发
1. 阅读 [开发指南 - 环境准备](development-guide.md#环境准备)
2. 阅读 [开发指南 - 开发流程](development-guide.md#开发流程)
3. 参考 [架构文档 - 核心模块](architecture.md#核心模块详解)

#### 调用后端 API
→ 查看 [API 参考文档](api-reference.md)

#### 调试问题
→ 参考 [开发指南 - 调试技巧](development-guide.md#调试技巧)

#### 构建发布版本
→ 阅读 [部署指南 - 本地构建](deployment-guide.md#本地构建)

#### 配置 CI/CD
→ 参考 [部署指南 - CI/CD 自动构建](deployment-guide.md#cicd-自动构建)

#### 添加新功能
1. 阅读 [架构文档 - 扩展性](architecture.md#扩展性)
2. 参考 [开发指南 - 代码规范](development-guide.md#代码规范)
3. 查看 [API 参考文档 - API 使用最佳实践](api-reference.md#api-使用最佳实践)

---

## 文档结构

```
docs/
├── README.md                  # 本文件 - 文档导航
├── architecture.md            # 架构文档
├── api-reference.md           # API 参考
├── development-guide.md       # 开发指南
└── deployment-guide.md        # 部署指南
```

---

## 项目概览

### 技术栈

**后端**:
- Python 3.11+
- Eel (Python/JavaScript 桥接)
- PyWebview (原生窗口)
- Aria2 (下载管理)
- Requests (HTTP 客户端)

**前端**:
- Vue 3 (Composition API)
- Vuetify 3 (UI 组件库)
- Pinia (状态管理)
- TypeScript
- Vite (构建工具)

**工具**:
- PyInstaller (打包)
- GitHub Actions (CI/CD)
- Sentry (错误追踪)

### 核心功能

- ✅ 多模拟器支持 (Ryujinx, Eden, Citron)
- ✅ 自动更新
- ✅ 固件管理
- ✅ 存档管理
- ✅ 金手指管理
- ✅ 多线程下载
- ✅ 镜像源支持

---

## 贡献文档

### 文档改进

如果您发现文档有误或需要改进:

1. **提交 Issue**:
   - 访问 [GitHub Issues](https://github.com/triwinds/ns-emu-tools/issues)
   - 使用标签 `documentation`

2. **提交 PR**:
   ```bash
   # 克隆仓库
   git clone https://github.com/triwinds/ns-emu-tools.git
   cd ns-emu-tools

   # 创建分支
   git checkout -b docs/improve-xxx

   # 修改文档
   # 编辑 docs/*.md

   # 提交
   git add docs/
   git commit -m "docs: 改进 XXX 文档"
   git push origin docs/improve-xxx
   ```

3. **文档规范**:
   - 使用 Markdown 格式
   - 保持清晰的层次结构
   - 添加代码示例
   - 包含实际用例
   - 更新目录

### 添加新文档

如需添加新的文档主题:

1. 在 `docs/` 目录创建新的 `.md` 文件
2. 在本 README 中添加链接
3. 更新相关文档的交叉引用
4. 提交 PR

---

## 外部资源

### 官方文档

- [Python 官方文档](https://docs.python.org/3/)
- [Vue 3 官方文档](https://vuejs.org/)
- [Vuetify 3 官方文档](https://vuetifyjs.com/)
- [Eel 文档](https://github.com/python-eel/Eel)
- [PyInstaller 文档](https://pyinstaller.org/)

### 相关项目

- [Ryujinx](https://ryujinx.app/) - Ryujinx 模拟器
- [Eden](https://eden-emu.dev/) - Eden 模拟器
- [Citron](https://citron-emu.org/) - Citron 模拟器
- [NSZ](https://github.com/nicoboss/nsz) - NS 固件解析工具
- [Aria2](https://aria2.github.io/) - 下载工具

### 社区

- **GitHub**: [triwinds/ns-emu-tools](https://github.com/triwinds/ns-emu-tools)
- **Telegram**: [讨论组](https://t.me/+mxI34BRClLUwZDcx)
- **Issues**: [问题追踪](https://github.com/triwinds/ns-emu-tools/issues)

---

## 版本历史

### 文档版本

| 版本 | 日期 | 说明 |
|------|------|------|
| 1.0 | 2025-12-18 | 初始版本，包含完整的架构、API、开发和部署文档 |

### 项目版本

当前项目版本: **0.5.9**

查看完整的版本历史: [CHANGELOG.md](../CHANGELOG.md)

---

## 许可证

本文档和项目代码均采用 [AGPL-3.0](../LICENSE) 许可证。

---

## 联系方式

### 获取帮助

- **文档问题**: 提交 [GitHub Issue](https://github.com/triwinds/ns-emu-tools/issues) (标签: `documentation`)
- **技术问题**: 提交 [GitHub Issue](https://github.com/triwinds/ns-emu-tools/issues) (标签: `question`)
- **Bug 报告**: 提交 [GitHub Issue](https://github.com/triwinds/ns-emu-tools/issues) (标签: `bug`)
- **功能请求**: 提交 [GitHub Issue](https://github.com/triwinds/ns-emu-tools/issues) (标签: `enhancement`)

### 讨论

加入我们的 [Telegram 讨论组](https://t.me/+mxI34BRClLUwZDcx) 与其他开发者和用户交流。

---

## 致谢

感谢所有为本项目和文档做出贡献的开发者！

特别感谢:
- 所有模拟器项目的开发者
- 开源社区的支持
- 用户的反馈和建议

---

**文档维护者**: triwinds
**最后更新**: 2025-12-18
**文档版本**: 1.0

---

<div align="center">

**[返回项目主页](../README.md)** | **[查看源代码](https://github.com/triwinds/ns-emu-tools)**

</div>
