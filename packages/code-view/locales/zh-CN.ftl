lsp-starting = { $language }：正在启动语言服务器…
lsp-ready = { $server }：就绪
lsp-exited = { $language }：语言服务器已退出
lsp-init-failed = { $language }：初始化失败：{ $error }
editor-rename-needs-lsp = 重命名需要此文件的语言服务器
editor-definition-outside = 定义位于文件夹之外：{ $uri }
editor-no-definition = 未找到定义
editor-applied = 已将修改应用到 { $n } 个文件
editor-edit-failed = 编辑失败：{ $error }
editor-nothing-to-rename = 此处没有可重命名的内容
editor-plain-text = 纯文本
editor-unsaved = 未保存的更改
editor-wrap-title = 自动换行
editor-wrap = 换行
editor-save = 保存
editor-reload-title = 放弃修改并从来源重新加载
editor-reload = 重新加载
editor-to-rust = 在 Rust 编辑器中显示此文件
editor-rename-prompt = 将 “{ $word }” 重命名为：
editor-rename = 重命名
editor-cancel = 取消
editor-no-actions = 此处没有代码操作。
editor-actions = 代码操作：
editor-references = { $n } 处引用
editor-conflict = 文件已在编辑器外被修改。重新加载可查看新内容（你的修改将丢失），或再次保存以覆盖。
editor-save-failed = 保存失败：{ $error }
editor-failed = 编辑器无法启动。
editor-failed-hint =  这是一个缺陷——请附上下面的信息报告（文件本身没有问题；点击“重新加载”可重试）。
editor-loading = 正在加载编辑器…

editor-to-codemirror = 在 CodeMirror 中显示此文件
editor-document-closed = 此文档已关闭。
editor-rust-meta = { $language } · v{ $version } · Rust 编辑器
editor-rust-label = 代码编辑器
editor-find = 查找
editor-match-case = 区分大小写
editor-whole-word = 全字匹配
editor-find-previous = 上一个
editor-find-next = 下一个
editor-replace = 替换
editor-replace-all = 全部替换
editor-toggle-comment = 切换注释
editor-fold = 折叠
editor-unfold = 展开
editor-fold-all = 全部折叠
editor-unfold-all = 全部展开

editor-search-regex = 正则表达式
editor-search-count = { $current } / { $total }
editor-search-captures-help = 使用 $1 或 $name 引用捕获组；$$ 插入美元符号。
editor-search-invalid = 无效的正则表达式：{ $error }

editor-diagnostics = 诊断
editor-hover = 显示悬停信息
editor-completion = 补全
editor-definition = 转到定义
editor-definition-needs-lsp = 转到定义需要此文件的语言服务器
editor-definition-failed = 转到定义失败：{ $error }
editor-definition-timeout = 转到定义超时

editor-rename-shortcut = 重命名符号
editor-rename-timeout = 重命名请求超时

editor-actions-shortcut = 代码操作
editor-references-shortcut = 查找引用
editor-tools-needs-lsp = 此操作需要此文件的语言服务器
editor-tools-timeout = 语言服务器请求超时
editor-tools-loading = 正在加载…
editor-reference-outside = 此引用位于文件夹之外

editor-preview = 预览
editor-preview-title = 预览 Markdown 格式；启用时自动换行。
editor-out-of-sync = 编辑器与文档不一致且无法重新同步，为保护文件已停止编辑。请关闭后重新打开；已保存的文件未改动。

lsp-restart-unavailable = 重启需要活动文件夹文档的语言服务器

editor-rename-stale = 文档已更改；请重新打开重命名

editor-link-failed = 无法打开链接：{ $error }
editor-link-heading-missing = 未找到标题：{ $heading }

editor-image-loading = 正在加载图片…
editor-image-missing = 找不到图片
editor-image-too-big = 图片超过 24 MiB 限制
editor-image-pixels = 图片尺寸超过像素限制
editor-image-failed = 无法显示图片
editor-math-failed = 无法显示此公式。点击编辑源文本。
editor-math-config = 已忽略无效或过大的 .moonkale/katex.json，使用默认数学宏。

editor-wiki-preview = Wiki 链接预览
editor-wiki-preview-loading = 正在加载笔记…
editor-wiki-preview-missing = 未找到笔记
editor-wiki-preview-unavailable = 预览不可用
editor-wiki-preview-too-large = 笔记超过 256 KiB 的预览限制

editor-properties = 属性
editor-properties-none = 无
editor-properties-yaml = 文档元数据（YAML）
editor-properties-apply = 应用
editor-properties-cancel = 取消
editor-properties-stale = 此草稿已过期。请取消并重新打开属性以编辑最新文档。
editor-properties-invalid = 请输入 YAML 属性，不要包含元数据分隔行。

editor-doc-preview = 文档预览
editor-doc-preview-title = 将文档注释显示为 Markdown；编辑预览时显示源码。

editor-a11y-help = 使用方向键阅读源码并移动光标。{ $primary }+M 切换 Tab 键的焦点移动和缩进功能。{ $primary }+Enter 打开光标处的链接或图片。Escape 关闭工具。
editor-a11y-position = 第 { $line } 行，第 { $column } 列。
editor-a11y-selection = 从第 { $line } 行第 { $column } 列选择到第 { $end_line } 行第 { $end_column } 列。
editor-a11y-tab-focus = Tab 移动焦点。
editor-a11y-tab-indent = Tab 缩进。

editor-group-history-blocked = 无法撤销或重做此重命名。请先撤销 { $file } 中较新的编辑，或协调历史记录已不可用。


native-reload = Rust 编辑器：重新加载
native-hover = Rust 编辑器：显示悬浮信息
native-complete = Rust 编辑器：补全
native-preview = Rust 编辑器：Markdown 预览
native-doc-preview = Rust 编辑器：文档预览
native-codemirror = Rust 编辑器：切换到 CodeMirror
native-indent-style = 缩进
native-indent-width = 宽度
native-default = 语言默认
native-spaces = 空格
native-tabs = 制表符

# CodeMirror search panel
cm-search-find = 查找
cm-search-replace = 替换
cm-search-next = 下一个
cm-search-previous = 上一个
cm-search-all = 全部
cm-search-case = 区分大小写
cm-search-regexp = 正则表达式
cm-search-word = 全字匹配
cm-search-replace-one = 替换
cm-search-replace-all = 全部替换
cm-search-close = 关闭
cm-search-goto = 跳转到行
cm-search-go = 跳转
cm-search-current = 当前匹配
cm-search-replaced = 已替换 $ 处
cm-search-replaced-line = 已替换第 $ 行匹配
cm-search-on-line = 在行
