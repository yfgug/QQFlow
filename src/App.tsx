import { Routes, Route, Navigate } from 'react-router-dom'
import TitleBar from './components/TitleBar'
import Sidebar from './components/Sidebar'
import HomePage from './pages/HomePage'
import KeyExtractPage from './pages/KeyExtractPage'
import DatabasePage from './pages/DatabasePage'
import ExportPage from './pages/ExportPage'
import AnalysisPage from './pages/AnalysisPage'
import SettingsPage from './pages/SettingsPage'
import { useState } from 'react'
import './App.scss'

function App() {
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false)

  return (
    <div className="app-root">
      <TitleBar
        title="QQFlow"
        sidebarCollapsed={sidebarCollapsed}
        onToggleSidebar={() => setSidebarCollapsed(!sidebarCollapsed)}
      />
      <div className="app-body">
        <Sidebar collapsed={sidebarCollapsed} />
        <main className="app-content">
          <Routes>
            <Route path="/" element={<Navigate to="/home" replace />} />
            <Route path="/home" element={<HomePage />} />
            <Route path="/key" element={<KeyExtractPage />} />
            <Route path="/database" element={<DatabasePage />} />
            <Route path="/export" element={<ExportPage />} />
            <Route path="/analysis" element={<AnalysisPage />} />
            <Route path="/settings" element={<SettingsPage />} />
          </Routes>
        </main>
      </div>
    </div>
  )
}

export default App
