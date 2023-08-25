# Dev

## 开发环境需求

- Python 3.11
- Node.js 18

## 运行环境准备

### Step 1 构建前端 package

```shell
cd vue3
yarn install
yarn build
```

### Step 2 安装 Python 依赖

```shell
# 通过 poetry 安装 (推荐)
poetry install

# 通过 pip 安装
python -m venv venv
venv\Scripts\activate
pip install -r requirements.txt
```

## 本地运行

```shell
poetry run python main.py
```

## 开发与调试

### 调试前端页面

先启动后端服务
```shell
poetry run python ui.py
```

然后另起一个终端启动 dev server
```shell
cd vue3
yarn dev
```

### 调试 Python 代码

由于 gevent 会与 pydebugger 冲突，因此不建议在 eel 启动时调试 Python 代码。

可以直接使用 pycharm 或 vscode 等 IDE 在 py 文件中通过 main 方法进行调试。

