// Keyboard shortcuts shown in the help dialog. Keep in sync with the key handlers in
// Reader.tsx, FlowView.tsx and PagedView.tsx.

const mac = navigator.platform.toLowerCase().includes("mac");
export const MOD = mac ? "⌘" : "Ctrl";

export const SHORTCUTS: { group: string; items: [string, string][] }[] = [
  {
    group: "翻页与移动",
    items: [
      ["← / →", "上一章 / 下一章（分页模式与 PDF 为上一页 / 下一页）"],
      ["空格 / PgDn", "向后翻一屏或一页，读到章末自动进入下一章"],
      ["Shift+空格 / PgUp", "向前翻一屏或一页"],
      ["↑ / ↓", "小幅滚动（单页模式翻页）"],
      ["Home / End", "章节或全书的开头 / 末尾"],
      [mac ? "⌥← / ⌥→" : "Alt+← / Alt+→", "后退 / 前进（回到跳转前的位置）"],
    ],
  },
  {
    group: "书签与笔记",
    items: [
      [`${MOD}+D`, "为当前位置添加或取消书签"],
      ["选中文字", "高亮、写笔记、复制"],
    ],
  },
  {
    group: "查看",
    items: [
      [`${MOD} + / ${MOD} -`, "调整字号（PDF 为缩放）"],
      [`${MOD}+F`, "全文搜索"],
      ["T", "显示 / 隐藏侧栏"],
      ["F", "专注模式"],
      [mac ? "⌃⌘F / F11" : "F11", "全屏"],
      ["?", "快捷键帮助"],
      ["Esc", "关闭面板，或返回书架"],
    ],
  },
];
