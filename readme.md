# Ns Emu Tools

一个用于安装/更新 NS 模拟器的工具

![GitHub release (latest by date)](https://img.shields.io/github/v/release/triwinds/ns-emu-tools?style=for-the-badge)
![GitHub last commit](https://img.shields.io/github/last-commit/triwinds/ns-emu-tools?style=for-the-badge)
![GitHub all releases](https://img.shields.io/github/downloads/triwinds/ns-emu-tools/total?style=for-the-badge)
![GitHub Repo stars](https://img.shields.io/github/stars/triwinds/ns-emu-tools?style=for-the-badge)
![GitHub](https://img.shields.io/github/license/triwinds/ns-emu-tools?style=for-the-badge)

## Features

 - 支持安装 Yuzu EA/正式 版模拟器
 - 支持 Yuzu 版本检测及更新
 - 支持安装 Ryujinx Ava/正式/LDN 版模拟器
 - 支持 Ryujinx 版本检测及更新
 - 自动检测并安装 msvc 运行库
 - 支持安装及更新 NS 固件至模拟器
 - 支持固件版本检测 (感谢 [a709560839](https://tieba.baidu.com/home/main?id=tb.1.f9804802.YmDokXJSRkAJB0xF8XfaCQ&fr=pb) 提供的思路)
 - 管理模拟器密钥
 - Yuzu 金手指管理
 - aria2 多线程下载

## 使用方法

### 一、使用预构建的版本运行

从 [GitHub 发布页(稳定版本)](https://github.com/triwinds/ns-emu-tools/releases) 或 
[CI 自动构建](https://github.com/triwinds/ns-emu-tools/actions/workflows/ci-build.yaml) 下载 exe 文件，然后双击运行即可。
<details>
<summary>NsEmuTools.exe 和 NsEmuTools-console.exe 有什么区别？</summary>
NsEmuTools.exe 和 NsEmuTools-console.exe 在实际的功能上并没有任何差异，
其主要的差别在于 console 会在启动的时候多一个命令行窗口，这也许可以解决某些杀毒软件的误报问题，
详情见 <a href="https://github.com/triwinds/ns-emu-tools/issues/2">#2</a>.
</details>


### 二、使用源码运行

参见 [开发文档](doc/dev.md)


## 讨论组

Telegram: [Telegram 讨论组](https://t.me/+mxI34BRClLUwZDcx)


## License

本项目的发布受 [AGPL-3.0](https://github.com/triwinds/ns-emu-tools/blob/main/LICENSE) 许可认证。

## 启动参数

```
usage: NsEmuTools-console.exe [-h] [-m {webview,browser,chrome,edge,user default}]
                              [--switch-mode {auto,webview,browser,chrome,edge,user default}]

options:
  -h, --help            show this help message and exit
  -m {webview,browser,chrome,edge,user default}, --mode {webview,browser,chrome,edge,user default}
                        指定 ui 启动方式
  --switch-mode {auto,webview,browser,chrome,edge,user default}
                        切换 ui 启动方式
```

## Credits

 - [Yuzu](https://github.com/yuzu-emu/yuzu) - Yuzu 模拟器
 - [Ryujinx](https://github.com/Ryujinx/Ryujinx) - Ryujinx 模拟器
 - [hactool](https://github.com/SciresM/hactool) - NS 固件解析
 - [aria2](https://github.com/aria2/aria2) - aria2 下载器
 - [Github 镜像源](https://github.com/XIU2/UserScript/blob/master/GithubEnhanced-High-Speed-Download.user.js) - 来自 X.I.U 大佬的 Github 增强脚本
 - [pineappleEA](https://github.com/pineappleEA/pineapple-src) - Yuzu EA 版本来源
 - [darthsternie.net](https://darthsternie.net/switch-firmwares/) - NS 固件来源
