import { Minus, Square, X, Copy, PanelLeftClose, PanelLeftOpen } from 'lucide-react'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { useEffect, useState } from 'react'
import './TitleBar.scss'

interface TitleBarProps {
  title?: string
  sidebarCollapsed?: boolean
  onToggleSidebar?: () => void
}

function TitleBar({ title, sidebarCollapsed, onToggleSidebar }: TitleBarProps) {
  const [isMaximized, setIsMaximized] = useState(false)
  const appWindow = getCurrentWindow()

  useEffect(() => {
    appWindow.isMaximized().then(setIsMaximized).catch(() => {})
    const unlisten = appWindow.onResized(async () => {
      const maximized = await appWindow.isMaximized()
      setIsMaximized(maximized)
    })
    return () => { unlisten.then(fn => fn()) }
  }, [])

  return (
    <div data-tauri-drag-region className="title-bar">
      <div className="title-brand">
        <span className="title-logo">Q</span>
        <span className="title-text">{title || 'QQFlow'}</span>
        {onToggleSidebar && (
          <button
            type="button"
            className="title-sidebar-toggle"
            onClick={onToggleSidebar}
            title={sidebarCollapsed ? '展开菜单' : '收起菜单'}
          >
            {sidebarCollapsed ? <PanelLeftOpen size={16} /> : <PanelLeftClose size={16} />}
          </button>
        )}
      </div>
      <div className="title-window-controls">
        <button
          type="button"
          className="title-window-control-btn"
          aria-label="最小化"
          onClick={() => appWindow.minimize()}
        >
          <Minus size={14} />
        </button>
        <button
          type="button"
          className="title-window-control-btn"
          aria-label={isMaximized ? '还原' : '最大化'}
          onClick={() => appWindow.toggleMaximize()}
        >
          {isMaximized ? <Copy size={12} /> : <Square size={12} />}
        </button>
        <button
          type="button"
          className="title-window-control-btn is-close"
          aria-label="关闭"
          onClick={() => appWindow.close()}
        >
          <X size={14} />
        </button>
      </div>
    </div>
  )
}

export default TitleBar
