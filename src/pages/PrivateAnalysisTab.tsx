import { useState, useEffect, useRef, useMemo } from 'react'
import ReactECharts from 'echarts-for-react'
import { Search } from 'lucide-react'
import { useAppStore } from '../stores/appStore'
import { api } from '../lib/api'

interface ContactInfo { id: string; name: string; messageCount: number }
interface PrivateData {
  totalMessages: number; activeDays: number; contactCount: number
  hourlyDistribution: Record<string, number>; weekdayDistribution: Record<string, number>
  monthlyDistribution: Record<string, number>; typeDistribution: Array<{ name: string; value: number }>
  contactRanking: Array<{ name: string; messageCount: number }>
  topPhrases: Array<{ phrase: string; count: number }>
  firstMessage: string; lastMessage: string
}

function PrivateAnalysisTab() {
  const { extractedKey, selectedDb } = useAppStore()
  const [contacts, setContacts] = useState<ContactInfo[]>([])
  const [selectedContact, setSelectedContact] = useState<string | null>(null)
  const [data, setData] = useState<PrivateData | null>(null)
  const [loading, setLoading] = useState(false)
  const [contactsLoading, setContactsLoading] = useState(false)
  const [error, setError] = useState('')
  const [elapsed, setElapsed] = useState(0)
  const [search, setSearch] = useState('')
  const elapsedRef = useRef<ReturnType<typeof setInterval> | null>(null)

  useEffect(() => {
    if (!extractedKey || !selectedDb) return
    loadContacts()
  }, [extractedKey, selectedDb])

  const loadContacts = async () => {
    setContactsLoading(true); setError('')
    try {
      const res = await api.analyzePrivate({ db_path: selectedDb!.path, key: extractedKey! })
      if (res.ok && res.data) {
        const cs = (res.data.groups || []) as ContactInfo[]
        cs.sort((a, b) => b.messageCount - a.messageCount)
        setContacts(cs)
        if (!cs.length) setError('未找到私聊数据')
      } else setError(res.error || '加载联系人列表失败')
    } catch (e: any) { setError(e.message || '请求失败') }
    setContactsLoading(false)
  }

  const loadContactDetail = async (peerId: string) => {
    setSelectedContact(peerId); setLoading(true); setElapsed(0)
    if (elapsedRef.current) { clearInterval(elapsedRef.current); elapsedRef.current = null }
    elapsedRef.current = setInterval(() => setElapsed(e => e + 1), 1000)

    try {
      const res = await api.analyzePrivate({ db_path: selectedDb!.path, key: extractedKey!, peer_id: peerId })
      if (res.ok && res.data) { setData(res.data) }
      else { setError(res.error || '分析失败'); setSelectedContact(null) }
    } catch (e: any) { setError(e.message || '请求失败'); setSelectedContact(null) }

    setLoading(false)
    if (elapsedRef.current) { clearInterval(elapsedRef.current); elapsedRef.current = null }
  }

  const filteredContacts = useMemo(() => {
    if (!search.trim()) return contacts
    const s = search.toLowerCase()
    return contacts.filter(c => c.id.includes(s) || c.name.toLowerCase().includes(s))
  }, [contacts, search])

  if (!extractedKey || !selectedDb) return <div className="analysis-empty"><p>请先完成密钥提取和数据库选择</p></div>
  if (contactsLoading) return <div className="loading">正在加载联系人列表...</div>
  if (error) return <div className="empty error-text">{error}</div>

  // ── Contact list view ──
  if (!selectedContact) {
    return (
      <div className="group-list-section">
        <h2>选择联系人</h2>
        {contacts.length > 0 && (
          <div className="search-box">
            <Search size={14} />
            <input type="text" placeholder="搜索联系人名称或ID..." value={search} onChange={e => setSearch(e.target.value)} />
          </div>
        )}
        {filteredContacts.length === 0 ? (
          <div className="empty">{search ? '没有匹配的联系人' : '未找到私聊数据'}</div>
        ) : (
          <div className="group-grid">
            {filteredContacts.map(c => (
              <button key={c.id} className="group-card" onClick={() => loadContactDetail(c.id)}>
                <div className="group-icon"><span>{c.name ? c.name.slice(0, 2) : c.id.slice(-2)}</span></div>
                <div className="group-info">
                  <div className="group-name">{c.name || c.id}</div>
                  <div className="group-count">{c.messageCount.toLocaleString()} 条消息</div>
                </div>
              </button>
            ))}
          </div>
        )}
      </div>
    )
  }

  // ── Loading ──
  if (loading) return (
    <div className="loading">
      <p>正在分析私聊数据...</p>
      <p className="elapsed">已运行 {elapsed} 秒{elapsed > 10 ? '，数据量大请耐心等待' : ''}</p>
    </div>
  )

  if (!data) return null

  // ── Detail view ──
  const contactName = contacts.find(c => c.id === selectedContact)?.name || selectedContact
  const hourlyOption = {
    title: { text: '24 小时消息分布', left: 'center', textStyle: { fontSize: 14 } },
    tooltip: { trigger: 'axis' as const },
    xAxis: { type: 'category' as const, data: Array.from({ length: 24 }, (_, i) => `${i}:00`) },
    yAxis: { type: 'value' as const },
    series: [{ type: 'bar' as const, data: Array.from({ length: 24 }, (_, i) => data.hourlyDistribution[String(i)] || 0), itemStyle: { color: '#10a37f', borderRadius: [4, 4, 0, 0] } }],
    grid: { left: 50, right: 20, bottom: 40, top: 50 },
  }
  const typeOption = {
    title: { text: '消息类型分布', left: 'center', textStyle: { fontSize: 14 } },
    tooltip: { trigger: 'item' as const },
    legend: { bottom: 0 },
    series: [{ type: 'pie' as const, radius: ['40%', '65%'], center: ['50%', '45%'], data: data.typeDistribution.filter(d => d.value > 0) }],
  }
  const weekdayOption = {
    title: { text: '星期分布', left: 'center', textStyle: { fontSize: 14 } },
    tooltip: { trigger: 'axis' as const },
    xAxis: { type: 'category' as const, data: Object.keys(data.weekdayDistribution) },
    yAxis: { type: 'value' as const },
    series: [{ type: 'bar' as const, data: Object.values(data.weekdayDistribution), itemStyle: { color: '#6366f1', borderRadius: [4, 4, 0, 0] } }],
    grid: { left: 50, right: 20, bottom: 40, top: 50 },
  }
  const monthlyOption = Object.keys(data.monthlyDistribution).length > 0 ? {
    title: { text: '月度趋势', left: 'center', textStyle: { fontSize: 14 } },
    tooltip: { trigger: 'axis' as const },
    xAxis: { type: 'category' as const, data: Object.keys(data.monthlyDistribution) },
    yAxis: { type: 'value' as const },
    series: [{ type: 'line' as const, data: Object.values(data.monthlyDistribution), smooth: true, areaStyle: { color: 'rgba(16,163,127,0.15)' }, lineStyle: { color: '#10a37f', width: 2 } }],
    grid: { left: 50, right: 20, bottom: 40, top: 50 },
  } : null

  return (
    <div className="analysis-detail">
      <div className="detail-header">
        <button className="back-btn" onClick={() => { setSelectedContact(null); setData(null) }}>&larr; 返回联系人列表</button>
        <h2>{contactName} 的私聊分析</h2>
        <p className="subtitle">{data.totalMessages.toLocaleString()} 条消息 &middot; {data.activeDays} 天活跃 &middot; {data.firstMessage} ~ {data.lastMessage}</p>
      </div>
      <div className="stats-cards">
        <div className="stat-card"><div className="stat-value">{data.totalMessages.toLocaleString()}</div><div className="stat-label">总消息数</div></div>
        <div className="stat-card"><div className="stat-value">{data.contactCount}</div><div className="stat-label">参与人数</div></div>
        <div className="stat-card"><div className="stat-value">{data.activeDays}</div><div className="stat-label">活跃天数</div></div>
        <div className="stat-card"><div className="stat-value">{data.contactRanking[0]?.name || '-'}</div><div className="stat-label">最活跃</div></div>
      </div>
      <div className="charts-grid">
        <div className="chart-card"><ReactECharts option={hourlyOption} style={{ height: 300 }} /></div>
        <div className="chart-card"><ReactECharts option={typeOption} style={{ height: 300 }} /></div>
        <div className="chart-card"><ReactECharts option={weekdayOption} style={{ height: 300 }} /></div>
        {monthlyOption && <div className="chart-card"><ReactECharts option={monthlyOption} style={{ height: 300 }} /></div>}
      </div>
      <div className="ranking-section">
        <h3>消息排行</h3>
        <div className="ranking-header"><span className="rank">#</span><span className="name">用户</span><span className="count">消息数</span><span className="bar"></span></div>
        {data.contactRanking.slice(0, 20).map((m, i) => (
          <div key={m.name} className="ranking-row">
            <span className="rank">{i + 1}</span><span className="name">{m.name}</span>
            <span className="count">{m.messageCount.toLocaleString()}</span>
            <span className="bar"><span className="bar-fill" style={{ width: `${(m.messageCount / data.contactRanking[0].messageCount) * 100}%` }} /></span>
          </div>
        ))}
      </div>
      {data.topPhrases.length > 0 && (
        <div className="phrases-section">
          <h3>高频短语</h3>
          <div className="phrase-cloud">
            {data.topPhrases.map((p, i) => (
              <span key={p.phrase} className="phrase-tag" style={{ fontSize: `${Math.max(12, 24 - i)}px` }}>{p.phrase} <small>({p.count})</small></span>
            ))}
          </div>
        </div>
      )}
    </div>
  )
}

export default PrivateAnalysisTab
