import { useState, useEffect } from 'react'
import { Database, RefreshCw, HardDrive, CheckCircle } from 'lucide-react'
import { useAppStore } from '../stores/appStore'
import { api } from '../lib/api'
import './DatabasePage.scss'

function DatabasePage() {
  const { databases, setDatabases, selectedDb, setSelectedDb } = useAppStore()
  const [scanning, setScanning] = useState(false)

  const scanDatabases = async () => {
    setScanning(true)
    const result = await api.scanDatabases()
    if (result.ok && result.databases) {
      setDatabases(result.databases)
      if (result.databases.length === 1) setSelectedDb(result.databases[0])
    }
    setScanning(false)
  }

  useEffect(() => {
    if (databases.length === 0) scanDatabases()
  }, [])

  return (
    <div className="database-page">
      <div className="page-header">
        <Database size={24} />
        <h1>数据库管理</h1>
      </div>
      <div className="page-content">
        <div className="db-action-bar">
          <button className="btn-primary" onClick={scanDatabases} disabled={scanning}>
            <RefreshCw size={16} className={scanning ? 'spin' : ''} />
            {scanning ? '扫描中...' : '扫描数据库'}
          </button>
        </div>
        {databases.length === 0 && !scanning && (
          <div className="db-empty">
            <HardDrive size={48} />
            <h3>未找到数据库</h3>
            <p>请确保已安装 QQ NT 并至少登录过一次</p>
            <p className="db-hint">数据库路径: %USERPROFILE%\Documents\Tencent Files\[QQ号]\nt_qq\nt_db\nt_msg.db</p>
          </div>
        )}
        {databases.length > 0 && (
          <div className="db-list">
            {databases.map((db, i) => (
              <div key={i} className={`db-card ${selectedDb?.path === db.path ? 'selected' : ''}`} onClick={() => setSelectedDb(db)}>
                <div className="db-card-icon"><HardDrive size={24} /></div>
                <div className="db-card-info">
                  <div className="db-card-qq">QQ: {db.qq}</div>
                  <div className="db-card-path">{db.path}</div>
                  <div className="db-card-size">{db.size_mb} MB</div>
                </div>
                {selectedDb?.path === db.path && <CheckCircle size={20} className="db-card-check" />}
              </div>
            ))}
          </div>
        )}
        {selectedDb && (
          <div className="db-selected-info">
            <CheckCircle size={16} />
            <span>已选择 QQ {selectedDb.qq} 的数据库 ({selectedDb.size_mb} MB)</span>
          </div>
        )}
      </div>
    </div>
  )
}

export default DatabasePage
