import { useState, useEffect, useRef } from 'react'
import { KeyRound, Play, CheckCircle, AlertCircle, Loader2, Trash2, Plus, Eye, EyeOff } from 'lucide-react'
import { useAppStore } from '../stores/appStore'
import { api } from '../lib/api'
import './KeyExtractPage.scss'

function KeyExtractPage() {
  const { savedKeys, setSavedKeys, databases, setDatabases, keyExtracting, setKeyExtracting, keyLogs, addKeyLog, clearKeyLogs } = useAppStore()
  const [selectedQq, setSelectedQq] = useState('')
  const [showKeys, setShowKeys] = useState<Record<string, boolean>>({})
  const logEndRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    logEndRef.current?.scrollIntoView({ behavior: 'smooth' })
  }, [keyLogs])

  // Load saved keys on mount
  useEffect(() => {
    api.loadKeys().then((result) => {
      if (result.ok && result.keys) {
        setSavedKeys(result.keys)
      }
    })
  }, [])

  // Auto-scan databases if empty
  useEffect(() => {
    if (databases.length === 0) {
      api.scanDatabases().then((result) => {
        if (result.ok && result.databases) {
          setDatabases(result.databases)
        }
      })
    }
  }, [])

  const handleExtract = async () => {
    if (!selectedQq) return
    setKeyExtracting(true)
    clearKeyLogs()

    const result = await api.extractKey()
    if (!result.ok) {
      addKeyLog({ level: 'error', text: result.error || '启动密钥提取失败' })
      setKeyExtracting(false)
      return
    }

    const poll = async () => {
      const keyStatus = await api.getKeyStatus()
      for (const msg of keyStatus.messages || []) {
        addKeyLog(msg)
      }
      if (keyStatus.done) {
        if (keyStatus.key) {
          if (selectedQq) {
            await api.saveKey(keyStatus.key, selectedQq)
            const keysResult = await api.loadKeys()
            if (keysResult.ok && keysResult.keys) {
              setSavedKeys(keysResult.keys)
            }
          } else {
            addKeyLog({ level: 'warn', text: '密钥已提取，但未选择 QQ 号，密钥未保存' })
          }
        }
        setKeyExtracting(false)
        return
      }
      setTimeout(poll, 500)
    }
    setTimeout(poll, 1000)
  }

  const handleDeleteKey = async (qq: string) => {
    await api.clearKey(qq)
    const keysResult = await api.loadKeys()
    if (keysResult.ok && keysResult.keys) {
      setSavedKeys(keysResult.keys)
    }
  }

  const toggleShowKey = (qq: string) => {
    setShowKeys(prev => ({ ...prev, [qq]: !prev[qq] }))
  }

  const copyKey = (key: string) => {
    navigator.clipboard.writeText(key)
  }

  const savedEntries = Object.entries(savedKeys)
  const availableQqs = databases.map(d => d.qq).filter(qq => !savedKeys[qq])

  return (
    <div className="key-page">
      <div className="page-header">
        <KeyRound size={24} />
        <h1>密钥提取</h1>
      </div>
      <div className="page-content">
        <div className="key-intro-card">
          <h2>自动提取加密密钥</h2>
          <p>QQ NT 使用 SQLCipher 加密聊天数据库。此工具通过 Windows 调试 API 自动从 QQ 进程中提取 16 位加密密钥。</p>
          <p className="key-note">提取过程中会自动启动 QQ，请在弹出的窗口中登录账号。</p>
        </div>

        {/* Saved keys list */}
        {savedEntries.length > 0 && (
          <div className="key-list-card">
            <h3>已保存的密钥 ({savedEntries.length})</h3>
            <div className="key-list">
              {savedEntries.map(([qq, key]) => (
                <div key={qq} className="key-item">
                  <div className="key-item-qq">
                    <KeyRound size={16} />
                    <span>QQ: {qq}</span>
                  </div>
                  <div className="key-item-actions">
                    <code className="key-item-code">
                      {showKeys[qq] ? key : '•'.repeat(16)}
                    </code>
                    <button className="btn-icon" onClick={() => toggleShowKey(qq)} title={showKeys[qq] ? '隐藏' : '显示'}>
                      {showKeys[qq] ? <EyeOff size={14} /> : <Eye size={14} />}
                    </button>
                    <button className="btn-icon" onClick={() => copyKey(key)} title="复制">
                      <KeyRound size={14} />
                    </button>
                    <button className="btn-icon btn-danger-icon" onClick={() => handleDeleteKey(qq)} title="删除">
                      <Trash2 size={14} />
                    </button>
                  </div>
                </div>
              ))}
            </div>
          </div>
        )}

        {/* Add new key */}
        <div className="key-action-card">
          <h3><Plus size={16} /> 添加新密钥</h3>
          {databases.length > 0 ? (
            <div className="key-extract-form">
              <div className="qq-select">
                <label>选择 QQ 号</label>
                <select value={selectedQq} onChange={(e) => setSelectedQq(e.target.value)}>
                  <option value="">-- 请选择 --</option>
                  {databases.map(db => (
                    <option key={db.qq} value={db.qq} disabled={!!savedKeys[db.qq]}>
                      {db.qq} {savedKeys[db.qq] ? '(已提取)' : ''}
                    </option>
                  ))}
                  {availableQqs.length === 0 && <option value="" disabled>所有账号均已提取</option>}
                </select>
              </div>
              <button
                className="btn-primary btn-large"
                onClick={handleExtract}
                disabled={keyExtracting || !selectedQq}
              >
                {keyExtracting ? (
                  <><Loader2 size={18} className="spin" /> 正在提取...</>
                ) : (
                  <><Play size={18} /> 开始提取密钥</>
                )}
              </button>
            </div>
          ) : (
            <div className="key-no-db">
              <p>未找到数据库。请确保已安装 QQ NT 并至少登录过一次。</p>
              <div className="qq-select">
                <label>手动输入 QQ 号（密钥保存需要）</label>
                <input
                  type="text"
                  className="qq-input"
                  placeholder="请输入 QQ 号"
                  value={selectedQq}
                  onChange={(e) => setSelectedQq(e.target.value)}
                />
              </div>
              <button
                className="btn-primary btn-large"
                onClick={handleExtract}
                disabled={keyExtracting || !selectedQq}
              >
                {keyExtracting ? (
                  <><Loader2 size={18} className="spin" /> 正在提取...</>
                ) : (
                  <><Play size={18} /> 开始提取密钥</>
                )}
              </button>
            </div>
          )}
        </div>

        {/* Extraction logs */}
        {keyLogs.length > 0 && (
          <div className="key-log-card">
            <h3>提取日志</h3>
            <div className="log-terminal">
              {keyLogs.map((msg, i) => (
                <div key={i} className={`log-line log-${msg.level}`}>
                  {msg.level === 'error' && <AlertCircle size={14} />}
                  {msg.level === 'success' && <CheckCircle size={14} />}
                  <span>{msg.text}</span>
                </div>
              ))}
              <div ref={logEndRef} />
            </div>
          </div>
        )}
      </div>
    </div>
  )
}

export default KeyExtractPage
