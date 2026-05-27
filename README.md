# 📄 PDF Toolkit

跨平台 PDF 处理桌面应用，基于 [Tauri 2.x](https://tauri.app) 构建。

## 功能

| 功能 | 说明 |
|------|------|
| 📎 合并 | 多个 PDF 合并为一个 |
| ✂️ 拆分 | 每页一个 / 按范围 / 从指定页一分为二 |
| 🗑️ 删除 | 指定页码范围删除（带预览确认） |
| 📋 提取 | 提取指定页面为新文件 |
| 🔄 旋转 | 90°/180°/270° 旋转 |
| ↕️ 重排序 | 按新顺序重排页面 |
| 📦 压缩 | 去除冗余，减小体积 |

## 下载

前往 [Releases](../../releases) 页面下载对应平台的安装包：

- **macOS**: `.dmg`
- **Windows**: `.msi` 或 `.exe`
- **Linux**: `.deb` 或 `.AppImage`

## 从源码构建

### 环境要求

- [Rust](https://rustup.rs/) (1.70+)
- [Node.js](https://nodejs.org/) (v18+)
- macOS: Xcode Command Line Tools

### 构建步骤

```bash
# 克隆仓库
git clone https://github.com/gcloudream/pdf-toolkit.git
cd pdf-toolkit

# 安装依赖
npm install

# 开发模式（热重载）
npm run tauri dev

# 构建发布包
npm run tauri build
```

构建产物位于 `src-tauri/target/release/bundle/`。

## 技术栈

- **前端**: Vite + TypeScript + 原生 CSS（暗色主题）
- **后端**: Rust + lopdf（纯 Rust PDF 处理，无外部依赖）
- **框架**: Tauri 2.x（原生 webview，~5MB 安装包）
- **CI/CD**: GitHub Actions（三平台自动构建）

## License

MIT
