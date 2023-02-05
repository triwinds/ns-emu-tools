# Change Log

## 0.2.8
 - 调整 ui 启动逻辑
 - 启动后自动创建 `切换 UI 启动模式.bat` 用于切换启动模式
 - 添加启动参数 `--switch-mode` 用于切换启动模式

## 0.2.7
 - 优化 CloudflareST 授权流程，仅在写入 hosts 时请求管理员权限
 - 修复在 windowed 打包方式下 CloudflareST 控制台显示不正常的问题
 - 访问 api 时默认启用 DNS over HTTPS (可在设置中关闭)
 - 指定 aria2 使用 Aliyun / DNSPod 的 DNS 服务器
 - 修复因路径大小写原因误删 Ryujinx 的 portable 文件夹的问题 (#23)
 - 合并 webview 进入 main.py

## 0.2.6
 - 新增试验性功能: Cloudflare 节点选优
 - 修复 yuzu mod 文件夹路径获取错误的问题 (#19)
 - 新增模拟器路径的历史记录 (#20)

## 0.2.5
 - webview 版本增加运行前环境检测，并自动下载缺失的组件
 - 替换不安全的 Unicode decode 方式
 - 新增配置项: 在启动 aria2 前自动删除旧的日志
 - 更新 UA 标识
 - 添加 `其它资源` 页面

ps. 现在的 webview 版本应该可以在没安装过 Microsoft Edge WebView2 的系统中运行了. 
如果您之前遇到过 webview 版本打不开的问题, 可以试试这个版本, 还有问题的话可以在 issue 中反馈.

## 0.2.4
 - 新增对 Ryujinx LDN 版本的支持 (#5)
 - 当 eel websocket 断开后在界面提示重启程序 (#16)
 - nodejs 版本更新至 18, 更新前端相关依赖的版本

## 0.2.3
 - 新增自动更新功能 (建议使用 webview 版本)
 - 当直连 GitHub api 出现问题时尝试使用 CDN 进行重试
 - `设置` 页面中新增开关 aria2 ipv6 的选项
 - `About` 页面中新增查看 更新日志 的按钮
 - `Ryujinx` 页面中新增查看 更新日志 的按钮
 - 更新缓存配置, 根据 HTTP 响应中的 Cache Control 进行缓存

### 关于 webview 版本

由于 js/css 语法的兼容性问题, 一些浏览器上可能无法正确展示页面, 所以这里提供一个使用 webview 打包的版本。

这个版本不依赖于用户环境中的浏览器, 而是使用 [Microsoft Edge WebView2](https://developer.microsoft.com/en-us/microsoft-edge/webview2/)
打开界面, 这个组件已经预置在较新版本的系统当中(通过 Windows Update 推送), 因此这些系统无需进行额外下载。如果你的系统中没有这个组件, 
可以从 [这里](https://developer.microsoft.com/zh-cn/microsoft-edge/webview2/#download-section) 下载。

此外, 由于浏览器的安全限制, 程序无法主动关闭打开的浏览器页面, 因此只有 webview 版本能在更新时自动关闭打开的窗口,
其余版本则需要手动关闭之前打开的页面。

## 0.2.2-fix
 - 修复未能正确转义 Yuzu 配置中的 Unicode 字符的问题 (#11)

## 0.2.2
 - 修复无法识别 Yuzu 自定义 nand/load 目录的问题 (#9)
 - 保存选择的主题 (#10)
 - 修复金手指文件使用大写后缀名时无法识别的问题

## 0.2.1
 - 更新 Edge 的检测机制，在无法检测到 Edge 时将尝试使用默认浏览器启动
 - 添加命令行启动参数，支持选择启动的浏览器 (chrome, edge, user default)
   - 例如强制使用默认浏览器启动 `NsEmuTools.exe -m "user default"`
 - 添加 常见问题 页面
 - 设置中添加更多的 GitHub 下载源选项
 - 更换游戏数据源
 - 修复 Yuzu 路径有特殊字符时无法检测版本的问题
 - 设置中添加选项，允许保留下载的文件 (#4)

## 0.2.0
 - 新增 Yuzu 金手指管理功能
 - 调整 aria2p 连接参数以修复某些情况下 aria2 接口调用失败的问题
 - 修复含有特殊字符路径时命令行无法执行的问题
 - 在修改模拟器目录时展示警告信息

## 0.1.9
 - aria2 禁用 ipv6
 - 新增网络设置相关选项
 - 添加 requests-cache 用于本地缓存 api 结果

## 0.1.8
 - 修复 windowed 打包方式无法正常启动 Edge 浏览器的问题

## 0.1.7
 - 基于 Vuetify 构建的新 UI
 - 添加 msvc 的代理源
 - 修复 Ryujinx 切换分支后由于版本相同导致无法开始下载的问题
 - 调整浏览器默认使用顺序: Chrome > Edge > User Default
