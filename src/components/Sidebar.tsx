import { NavLink, useLocation } from 'react-router-dom'
import { Home, KeyRound, Database, Download, BarChart3, Settings } from 'lucide-react'
import './Sidebar.scss'

interface SidebarProps {
  collapsed: boolean
}

function Sidebar({ collapsed }: SidebarProps) {
  const location = useLocation()

  const isActive = (path: string) =>
    location.pathname === path || location.pathname.startsWith(`${path}/`)

  return (
    <aside className={`sidebar ${collapsed ? 'collapsed' : ''}`}>
      <nav className="nav-menu">
        <NavLink to="/home" className={`nav-item ${isActive('/home') ? 'active' : ''}`} title={collapsed ? '首页' : undefined}>
          <span className="nav-icon"><Home size={20} /></span>
          <span className="nav-label">首页</span>
        </NavLink>
        <NavLink to="/key" className={`nav-item ${isActive('/key') ? 'active' : ''}`} title={collapsed ? '密钥提取' : undefined}>
          <span className="nav-icon"><KeyRound size={20} /></span>
          <span className="nav-label">密钥提取</span>
        </NavLink>
        <NavLink to="/database" className={`nav-item ${isActive('/database') ? 'active' : ''}`} title={collapsed ? '数据库' : undefined}>
          <span className="nav-icon"><Database size={20} /></span>
          <span className="nav-label">数据库</span>
        </NavLink>
        <NavLink to="/export" className={`nav-item ${isActive('/export') ? 'active' : ''}`} title={collapsed ? '导出' : undefined}>
          <span className="nav-icon"><Download size={20} /></span>
          <span className="nav-label">导出</span>
        </NavLink>
        <NavLink to="/analysis" className={`nav-item ${isActive('/analysis') ? 'active' : ''}`} title={collapsed ? '聊天分析' : undefined}>
          <span className="nav-icon"><BarChart3 size={20} /></span>
          <span className="nav-label">聊天分析</span>
        </NavLink>
      </nav>
      <div className="sidebar-footer">
        <NavLink to="/settings" className={`nav-item ${isActive('/settings') ? 'active' : ''}`} title={collapsed ? '设置' : undefined}>
          <span className="nav-icon"><Settings size={20} /></span>
          <span className="nav-label">设置</span>
        </NavLink>
      </div>
    </aside>
  )
}

export default Sidebar
