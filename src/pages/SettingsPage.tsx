import { useState } from 'react'
import { useThemeStore } from '../stores/themeStore'
import { useAppStore } from '../stores/appStore'
import { api } from '../lib/api'
import { Settings, Sun, Moon, Info, Bug, AlertTriangle } from 'lucide-react'
import './SettingsPage.scss'

function SettingsPage() {
  const { mode, toggleMode } = useThemeStore()
  const { extractedKey, selectedDb } = useAppStore()
  const [schemaInfo, setSchemaInfo] = useState<any>(null)
  const [schemaLoading, setSchemaLoading] = useState(false)

  const handleDebugSchema = async () => {
    if (!extractedKey || !selectedDb) return
    setSchemaLoading(true)
    try {
      const res = await api.debugDbSchema({ db_path: selectedDb.path, key: extractedKey })
      if (res.ok) setSchemaInfo(res.data)
      else setSchemaInfo({ error: res.error })
    } catch (e: any) {
      setSchemaInfo({ error: e.message })
    }
    setSchemaLoading(false)
  }

  return (
    <div className="settings-page">
      <div className="page-header">
        <Settings size={24} />
        <h1>设置</h1>
      </div>
      <div className="page-content">
        <div className="settings-section">
          <h2>外观</h2>
          <div className="settings-card">
            <div className="setting-item">
              <div className="setting-info">
                {mode === 'light' ? <Sun size={18} /> : <Moon size={18} />}
                <div>
                  <div className="setting-title">主题模式</div>
                  <div className="setting-desc">当前: {mode === 'light' ? '浅色' : '深色'}</div>
                </div>
              </div>
              <button className="btn-secondary" onClick={toggleMode}>切换到{mode === 'light' ? '深色' : '浅色'}</button>
            </div>
          </div>
        </div>
        <div className="settings-section">
          <h2>调试工具</h2>
          <div className="settings-card">
            <div className="setting-item">
              <div className="setting-info">
                <Bug size={18} />
                <div>
                  <div className="setting-title">数据库结构检测</div>
                  <div className="setting-desc">查看数据库表结构、行数和 UID 映射样本</div>
                </div>
              </div>
              <button className="btn-secondary" onClick={handleDebugSchema} disabled={schemaLoading || !extractedKey || !selectedDb}>
                {schemaLoading ? '检测中...' : '检测'}
              </button>
            </div>
            {schemaInfo && (
              <pre className="debug-output" style={{ marginTop: 12, padding: 12, background: 'var(--bg-secondary, #f5f5f5)', borderRadius: 8, fontSize: 12, maxHeight: 400, overflow: 'auto', whiteSpace: 'pre-wrap', wordBreak: 'break-all' }}>
                {JSON.stringify(schemaInfo, null, 2)}
              </pre>
            )}
          </div>
        </div>
        <div className="settings-section">
          <h2>风险提示</h2>
          <div className="settings-card">
            <div className="setting-item" style={{ alignItems: 'flex-start' }}>
              <div className="setting-info" style={{ alignItems: 'flex-start' }}>
                <AlertTriangle size={18} style={{ color: '#f59e0b', flexShrink: 0, marginTop: 2 }} />
                <div>
                  <div className="setting-title">使用须知</div>
                  <div className="setting-desc" style={{ lineHeight: 1.6, marginTop: 4 }}>
                    <p style={{ margin: '0 0 8px' }}>本工具仅供<strong>个人学习、研究和数据备份</strong>用途。</p>
                    <p style={{ margin: '0 0 8px' }}>使用前请确保您有权访问和处理相关数据。请遵守当地法律法规，严禁用于非法用途。导出的聊天记录可能包含他人个人信息，请妥善保管，未经授权不得传播。</p>
                    <p style={{ margin: '0 0 8px' }}>本工具通过 Windows Debug API 提取密钥，可能触发安全软件报警。密钥提取过程中会临时调试 QQ 进程，请确保 QQ 已关闭。</p>
                    <p style={{ margin: 0 }}>本工具按"现状"提供，作者不对使用后果负责。使用即表示您同意上述条款。</p>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </div>
        <div className="settings-section">
          <h2>关于</h2>
          <div className="settings-card">
            <div className="setting-item">
              <div className="setting-info">
                <Info size={18} />
                <div>
                  <div className="setting-title">QQFlow</div>
                  <div className="setting-desc">v1.1.5 (Rust + Tauri) — QQ 聊天记录本地解密导出工具</div>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  )
}

export default SettingsPage
