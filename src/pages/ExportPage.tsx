import { useState, useRef, useEffect } from 'react'
import { Download, Play, CheckCircle, AlertCircle, FolderOpen, Loader2, Search, CheckSquare, Square } from 'lucide-react'
import { useAppStore } from '../stores/appStore'
import { api } from '../lib/api'
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import { open as openShell, Command } from '@tauri-apps/plugin-shell'
import './ExportPage.scss'

interface GroupInfo { id: string; name: string; messageCount: number }

function ExportPage() {
  const { extractedKey, selectedDb, exporting, setExporting, exportDone, setExportDone, exportLogs, addExportLog, addExportLogs, clearExportLogs, exportSummary, setExportSummary } = useAppStore()
  const [exportFormat, setExportFormat] = useState<'txt' | 'csv'>('txt')
  const [outputDir, setOutputDir] = useState('')
  const [elapsed, setElapsed] = useState(0)
  const [groups, setGroups] = useState<GroupInfo[]>([])
  const [selectedGroups, setSelectedGroups] = useState<Set<string>>(new Set())
  const [contacts, setContacts] = useState<GroupInfo[]>([])
  const [selectedContacts, setSelectedContacts] = useState<Set<string>>(new Set())
  const [groupsLoading, setGroupsLoading] = useState(false)
  const [contactsLoading, setContactsLoading] = useState(false)
  const [search, setSearch] = useState('')
  const [contactSearch, setContactSearch] = useState('')
  const logEndRef = useRef<HTMLDivElement>(null)
  const pollingRef = useRef(false)
  const elapsedRef = useRef<ReturnType<typeof setInterval> | null>(null)

  useEffect(() => {
    if (exporting) {
      setElapsed(0)
      elapsedRef.current = setInterval(() => setElapsed(e => e + 1), 1000)
    } else {
      if (elapsedRef.current) { clearInterval(elapsedRef.current); elapsedRef.current = null }
    }
    return () => { if (elapsedRef.current) clearInterval(elapsedRef.current) }
  }, [exporting])

  useEffect(() => {
    logEndRef.current?.scrollIntoView({ behavior: 'smooth' })
  }, [exportLogs])

  const canExport = !!extractedKey && !!selectedDb

  // Load group list when DB is selected
  useEffect(() => {
    if (!canExport) { setGroups([]); setSelectedGroups(new Set()); return }
    setGroupsLoading(true)
    api.analyzeGroup({ db_path: selectedDb!.path, key: extractedKey! })
      .then(res => {
        if (res.ok && res.data?.groups) {
          const gs = res.data.groups as GroupInfo[]
          gs.sort((a, b) => b.messageCount - a.messageCount)
          setGroups(gs)
        }
      })
      .catch(() => {})
      .finally(() => setGroupsLoading(false))
  }, [canExport, selectedDb?.path, extractedKey])

  // Load contact list when DB is selected
  useEffect(() => {
    if (!canExport) { setContacts([]); setSelectedContacts(new Set()); return }
    setContactsLoading(true)
    api.analyzePrivate({ db_path: selectedDb!.path, key: extractedKey! })
      .then(res => {
        if (res.ok && res.data?.groups) {
          const cs = res.data.groups as GroupInfo[]
          cs.sort((a, b) => b.messageCount - a.messageCount)
          setContacts(cs)
        }
      })
      .catch(() => {})
      .finally(() => setContactsLoading(false))
  }, [canExport, selectedDb?.path, extractedKey])

  // Filter groups by search
  const filteredGroups = groups.filter(g => {
    if (!search.trim()) return true
    const s = search.toLowerCase()
    return g.id.includes(s) || g.name.toLowerCase().includes(s)
  })

  // Filter contacts by search
  const filteredContacts = contacts.filter(c => {
    if (!contactSearch.trim()) return true
    const s = contactSearch.toLowerCase()
    return c.id.includes(s) || c.name.toLowerCase().includes(s)
  })

  const toggleGroup = (id: string) => {
    setSelectedGroups(prev => {
      const next = new Set(prev)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })
  }

  const toggleContact = (id: string) => {
    setSelectedContacts(prev => {
      const next = new Set(prev)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })
  }

  const selectAllGroups = () => setSelectedGroups(new Set(filteredGroups.map(g => g.id)))
  const deselectAllGroups = () => setSelectedGroups(new Set())
  const selectAllContacts = () => setSelectedContacts(new Set(filteredContacts.map(c => c.id)))
  const deselectAllContacts = () => setSelectedContacts(new Set())

  const handleSelectFolder = async () => {
    const path = await openDialog({ directory: true, title: '选择导出目录' })
    if (path) setOutputDir(path)
  }

  const buildParams = (): any => {
    const p: any = { db_path: selectedDb!.path, key: extractedKey! }
    if (outputDir) p.output_dir = outputDir
    // 始终传递选中的 ID 列表（空列表 = 不导出）
    p.group_ids = Array.from(selectedGroups)
    p.peer_ids = Array.from(selectedContacts)
    return p
  }

  const handleExport = async () => {
    if (!extractedKey || !selectedDb) return
    if (exportFormat === 'csv') { await handleCsvExport(); return }

    setExporting(true)
    setExportDone(false)
    clearExportLogs()
    setExportSummary(null)
    pollingRef.current = true

    const params = buildParams()
    const result = await api.startExport(params)
    if (!result.ok) {
      addExportLog({ level: 'error', text: result.error || '启动导出失败' })
      setExporting(false)
      pollingRef.current = false
      return
    }

    let pollCount = 0
    const poll = async () => {
      if (!pollingRef.current) return
      pollCount++
      try {
        const status = await api.getExportStatus()
        if (status.messages?.length) addExportLogs(status.messages)
        if (status.done) {
          pollingRef.current = false
          setExportDone(true)
          setExporting(false)
          if (status.summary) setExportSummary(status.summary)
          return
        }
        if (pollCount >= 600) {
          pollingRef.current = false
          addExportLog({ level: 'error', text: '导出超时' })
          setExporting(false)
          return
        }
        setTimeout(poll, 500)
      } catch {
        if (pollCount < 600) setTimeout(poll, 1000)
        else { pollingRef.current = false; setExporting(false) }
      }
    }
    setTimeout(poll, 500)
  }

  const handleCsvExport = async () => {
    if (!extractedKey || !selectedDb) return
    setExporting(true)
    setExportDone(false)
    clearExportLogs()
    setExportSummary(null)
    pollingRef.current = true

    const params = buildParams()

    api.exportCsv(params).then((result) => {
      if (result.ok) {
        addExportLog({ level: 'success', text: `CSV 导出完成` })
        setExportSummary({
          total: result.total || 0,
          groups: result.groups || 0,
          private: result.private || 0,
          dir: result.dir || '',
        })
        setExportDone(true)
      } else {
        addExportLog({ level: 'error', text: result.error || 'CSV 导出失败' })
      }
      setExporting(false)
    }).catch((e: any) => {
      addExportLog({ level: 'error', text: e.message || 'CSV 导出失败' })
      setExporting(false)
    })
  }

  const handleReset = () => {
    pollingRef.current = false
    clearExportLogs()
    setExportDone(false)
    setExportSummary(null)
  }

  const handleOpenDir = async () => {
    const dir = exportSummary?.dir || outputDir
    if (!dir) return
    try {
      await openShell(dir)
    } catch {
      try {
        await Command.create('explorer', dir).execute()
      } catch (e: any) {
        addExportLog({ level: 'error', text: `打开目录失败: ${e.message || e}` })
      }
    }
  }

  const groupSelectedCount = selectedGroups.size
  const contactSelectedCount = selectedContacts.size

  return (
    <div className="export-page">
      <div className="page-header">
        <Download size={24} />
        <h1>导出聊天记录</h1>
      </div>
      <div className="page-content">
        {!canExport && (
          <div className="export-prereq">
            <AlertCircle size={20} />
            <div>
              <p>请先完成以下步骤:</p>
              <ul>
                {!extractedKey && <li>提取加密密钥</li>}
                {!selectedDb && <li>选择数据库</li>}
              </ul>
            </div>
          </div>
        )}
        {canExport && !exporting && !exportDone && (
          <div className="export-start-card">
            <h2>准备导出</h2>
            <div className="export-info">
              <div className="export-info-item"><span className="label">QQ 号:</span><span className="value">{selectedDb?.qq}</span></div>
              <div className="export-info-item"><span className="label">数据库大小:</span><span className="value">{selectedDb?.size_mb} MB</span></div>
              <div className="export-info-item"><span className="label">密钥:</span><span className="value">{extractedKey ? '••••••••' : '未提取'}</span></div>
            </div>

            {/* Group selection */}
            <div className="export-select-tabs">
              {groups.length > 0 && (
                <div className="group-select-section">
                  <div className="group-select-header">
                    <label>群聊（{groupSelectedCount}/{groups.length}）</label>
                    <div className="group-select-actions">
                      <button className="btn-link" onClick={selectAllGroups}>全选</button>
                      <button className="btn-link" onClick={deselectAllGroups}>取消全选</button>
                    </div>
                  </div>
                  <div className="search-box">
                    <Search size={14} />
                    <input
                      type="text"
                      placeholder="搜索群名称或群号..."
                      value={search}
                      onChange={e => setSearch(e.target.value)}
                    />
                  </div>
                  <div className="group-check-list">
                    {filteredGroups.length === 0
                      ? <p className="empty-hint">{search ? '没有匹配的群聊' : '未找到群聊'}</p>
                      : filteredGroups.map(g => (
                        <label key={g.id} className="group-check-item">
                          <span className="check-box" onClick={() => toggleGroup(g.id)}>
                            {selectedGroups.has(g.id) ? <CheckSquare size={16} /> : <Square size={16} />}
                          </span>
                          <span className="group-check-name">{g.name || `群 ${g.id}`}</span>
                          <span className="group-check-count">{g.messageCount.toLocaleString()} 条</span>
                        </label>
                      ))
                    }
                  </div>
                </div>
              )}
              {groupsLoading && <p className="loading-hint">加载群列表...</p>}

              {/* Contact (private chat) selection */}
              {contacts.length > 0 && (
                <div className="group-select-section">
                  <div className="group-select-header">
                    <label>私聊（{contactSelectedCount}/{contacts.length}）</label>
                    <div className="group-select-actions">
                      <button className="btn-link" onClick={selectAllContacts}>全选</button>
                      <button className="btn-link" onClick={deselectAllContacts}>取消全选</button>
                    </div>
                  </div>
                  <div className="search-box">
                    <Search size={14} />
                    <input
                      type="text"
                      placeholder="搜索联系人名称或ID..."
                      value={contactSearch}
                      onChange={e => setContactSearch(e.target.value)}
                    />
                  </div>
                  <div className="group-check-list">
                    {filteredContacts.length === 0
                      ? <p className="empty-hint">{contactSearch ? '没有匹配的联系人' : '未找到私聊数据'}</p>
                      : filteredContacts.map(c => (
                        <label key={c.id} className="group-check-item">
                          <span className="check-box" onClick={() => toggleContact(c.id)}>
                            {selectedContacts.has(c.id) ? <CheckSquare size={16} /> : <Square size={16} />}
                          </span>
                          <span className="group-check-name">{c.name || c.id}</span>
                          <span className="group-check-count">{c.messageCount.toLocaleString()} 条</span>
                        </label>
                      ))
                    }
                  </div>
                </div>
              )}
              {contactsLoading && <p className="loading-hint">加载联系人列表...</p>}
            </div>

            <div className="format-select">
              <label>导出格式</label>
              <div className="format-options">
                <button className={`format-btn ${exportFormat === 'txt' ? 'active' : ''}`} onClick={() => setExportFormat('txt')}>TXT 文本</button>
                <button className={`format-btn ${exportFormat === 'csv' ? 'active' : ''}`} onClick={() => setExportFormat('csv')}>CSV 表格</button>
              </div>
              <p className="format-hint">{exportFormat === 'txt' ? '按会话导出为纯文本文件，方便阅读' : '导出为 CSV 表格，可用 Excel 打开'}</p>
            </div>
            <div className="folder-select">
              <label>导出位置</label>
              <div className="folder-row">
                <input type="text" className="folder-input" value={outputDir} placeholder="默认: 项目目录下的 output 文件夹" readOnly />
                <button className="btn-secondary" onClick={handleSelectFolder}><FolderOpen size={16} /> 选择</button>
              </div>
            </div>
            <button className="btn-primary btn-large" onClick={handleExport}>
              <Play size={18} /> 开始导出（{groupSelectedCount} 个群聊, {contactSelectedCount} 个私聊）
            </button>
          </div>
        )}
        {exporting && (
          <div className="export-progress-card">
            <div className="export-progress-header">
              <Loader2 size={20} className="spin" />
              <span>正在导出...</span>
              <span className="elapsed">({elapsed}秒{elapsed > 10 ? '，数据量大请耐心等待' : ''})</span>
            </div>
            <button className="btn-secondary" style={{marginTop:12}} onClick={() => {
              api.cancelExport()
              pollingRef.current = false
              setExporting(false)
              addExportLog({ level: 'warn', text: '导出已取消' })
            }}>取消导出</button>
          </div>
        )}
        {exportLogs.length > 0 && (
          <div className="export-log-card">
            <h3>导出日志</h3>
            <div className="log-terminal">
              {exportLogs.slice(-100).map((msg, i) => (
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
        {exportDone && exportSummary && (
          <div className="export-result-card">
            <div className="export-result-header"><CheckCircle size={24} className="icon-success" /><h2>导出完成</h2></div>
            <div className="export-stats">
              <div className="stat-item"><div className="stat-number">{exportSummary.total.toLocaleString()}</div><div className="stat-label">总消息数</div></div>
              <div className="stat-item"><div className="stat-number">{exportSummary.groups}</div><div className="stat-label">群聊</div></div>
              <div className="stat-item"><div className="stat-number">{exportSummary.private}</div><div className="stat-label">私聊</div></div>
            </div>
            <div className="export-actions">
              <button className="btn-primary" onClick={handleOpenDir}><FolderOpen size={16} /> 打开导出目录</button>
              <button className="btn-secondary" onClick={handleReset}>重新导出</button>
            </div>
          </div>
        )}
      </div>
    </div>
  )
}

export default ExportPage
