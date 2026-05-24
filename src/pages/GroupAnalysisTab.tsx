import { useState, useEffect, useRef, useMemo } from 'react'
import ReactECharts from 'echarts-for-react'
import { Search } from 'lucide-react'
import { useAppStore } from '../stores/appStore'
import { api } from '../lib/api'

interface GroupInfo { id: string; name: string; messageCount: number }
interface GroupData {
  groupId: string; totalMessages: number; memberCount: number
  memberRanking: Array<{ name: string; messageCount: number }>
  hourlyDistribution: Record<string, number>; weekdayDistribution: Record<string, number>
  monthlyDistribution: Record<string, number>; typeDistribution: Array<{ name: string; value: number }>
  topPhrases: Array<{ phrase: string; count: number }>; firstMessage: string; lastMessage: string
}

function GroupAnalysisTab() {
  const { extractedKey, selectedDb } = useAppStore()
  const [groups, setGroups] = useState<GroupInfo[]>([])
  const [selectedGroup, setSelectedGroup] = useState<string | null>(null)
  const [data, setData] = useState<GroupData | null>(null)
  const [loading, setLoading] = useState(false)
  const [groupsLoading, setGroupsLoading] = useState(false)
  const [error, setError] = useState('')
  const [elapsed, setElapsed] = useState(0)
  const [search, setSearch] = useState('')
  const elapsedRef = useRef<ReturnType<typeof setInterval> | null>(null)

  const filteredGroups = useMemo(() => {
    if (!search.trim()) return groups
    const s = search.toLowerCase()
    return groups.filter(g => g.id.includes(s) || g.name.toLowerCase().includes(s))
  }, [groups, search])

  useEffect(() => {
    if (!extractedKey || !selectedDb) return
    loadGroups()
  }, [extractedKey, selectedDb])

  const loadGroups = async () => {
    setGroupsLoading(true); setError('')
    try {
      const res = await api.analyzeGroup({ db_path: selectedDb!.path, key: extractedKey! })
      if (res.ok && res.data) {
        setGroups(res.data.groups || [])
        if (!(res.data.groups || []).length) setError('未找到群聊数据')
      } else setError(res.error || '加载群聊列表失败')
    } catch (e: any) { setError(e.message || '请求失败') }
    setGroupsLoading(false)
  }

  const loadGroupDetail = async (groupId: string) => {
    setSelectedGroup(groupId); setLoading(true); setElapsed(0)
    if (elapsedRef.current) { clearInterval(elapsedRef.current); elapsedRef.current = null }
    elapsedRef.current = setInterval(() => setElapsed(e => e + 1), 1000)

    try {
      const res = await api.analyzeGroup({ db_path: selectedDb!.path, key: extractedKey!, group_id: groupId })
      if (res.ok && res.data) {
        setData(res.data)
      } else {
        setError(res.error || '分析失败')
        setSelectedGroup(null)
      }
    } catch (e: any) {
      setError(e.message || '请求失败')
      setSelectedGroup(null)
    }

    setLoading(false)
    if (elapsedRef.current) { clearInterval(elapsedRef.current); elapsedRef.current = null }
  }

  if (!extractedKey || !selectedDb) return <div className="analysis-empty"><p>请先完成密钥提取和数据库选择</p></div>
  if (groupsLoading) return <div className="loading">正在加载群列表...</div>
  if (error) return <div className="empty error-text">{error}</div>

  const hourlyOption = data ? {
    title: { text: '24 小时消息分布', left: 'center', textStyle: { fontSize: 14 } },
    tooltip: { trigger: 'axis' as const },
    xAxis: { type: 'category' as const, data: Array.from({ length: 24 }, (_, i) => `${i}:00`) },
    yAxis: { type: 'value' as const },
    series: [{ type: 'bar' as const, data: Array.from({ length: 24 }, (_, i) => data.hourlyDistribution[String(i)] || 0), itemStyle: { color: '#10a37f', borderRadius: [4, 4, 0, 0] } }],
    grid: { left: 50, right: 20, bottom: 40, top: 50 },
  } : null

  const typeOption = data ? {
    title: { text: '消息类型分布', left: 'center', textStyle: { fontSize: 14 } },
    tooltip: { trigger: 'item' as const, formatter: '{b}: {c} ({d}%)' },
    legend: { bottom: 0 },
    series: [{ type: 'pie' as const, radius: ['40%', '65%'], center: ['50%', '45%'], data: data.typeDistribution.filter(d => d.value > 0) }],
  } : null

  const weekdayOption = data ? {
    title: { text: '星期分布', left: 'center', textStyle: { fontSize: 14 } },
    tooltip: { trigger: 'axis' as const },
    xAxis: { type: 'category' as const, data: Object.keys(data.weekdayDistribution) },
    yAxis: { type: 'value' as const },
    series: [{ type: 'bar' as const, data: Object.values(data.weekdayDistribution), itemStyle: { color: '#6366f1', borderRadius: [4, 4, 0, 0] } }],
    grid: { left: 50, right: 20, bottom: 40, top: 50 },
  } : null

  const monthlyOption = data && Object.keys(data.monthlyDistribution).length > 0 ? {
    title: { text: '月度趋势', left: 'center', textStyle: { fontSize: 14 } },
    tooltip: { trigger: 'axis' as const },
    xAxis: { type: 'category' as const, data: Object.keys(data.monthlyDistribution) },
    yAxis: { type: 'value' as const },
    series: [{ type: 'line' as const, data: Object.values(data.monthlyDistribution), smooth: true, areaStyle: { color: 'rgba(16,163,127,0.15)' }, lineStyle: { color: '#10a37f', width: 2 }, itemStyle: { color: '#10a37f' } }],
    grid: { left: 50, right: 20, bottom: 40, top: 50 },
  } : null

  if (!selectedGroup) {
    return (
      <div className="group-list-section">
        <h2>选择群聊</h2>
        {groups.length > 0 && (
          <div className="search-box">
            <Search size={14} />
            <input
              type="text"
              placeholder="搜索群名称或群号..."
              value={search}
              onChange={e => setSearch(e.target.value)}
            />
          </div>
        )}
        {filteredGroups.length === 0 ? (
          <div className="empty">{search ? '没有匹配的群聊' : (groups.length === 0 ? '未找到群聊数据' : '')}</div>
        ) : (
          <div className="group-grid">
            {filteredGroups.map(g => (
              <button key={g.id} className="group-card" onClick={() => loadGroupDetail(g.id)}>
                <div className="group-icon"><span>{g.name ? g.name.slice(0, 2) : g.id.slice(-2)}</span></div>
                <div className="group-info">
                  <div className="group-name">{g.name || `群 ${g.id}`}</div>
                  <div className="group-count">{g.messageCount.toLocaleString()} 条消息</div>
                </div>
              </button>
            ))}
          </div>
        )}
      </div>
    )
  }

  if (loading) return (
    <div className="loading">
      <p>正在分析群聊数据...</p>
      <p className="elapsed">已运行 {elapsed} 秒{elapsed > 10 ? '（请查看桌面 qqflow_debug.txt 了解进度）' : ''}</p>
    </div>
  )
  if (!data) return null

  return (
    <div className="analysis-detail">
      <div className="detail-header">
        <button className="back-btn" onClick={() => { setSelectedGroup(null); setData(null) }}>&larr; 返回群列表</button>
        <h2>{groups.find(g => g.id === data.groupId)?.name || `群 ${data.groupId}`} 分析</h2>
        <p className="subtitle">{data.totalMessages.toLocaleString()} 条消息 &middot; {data.memberCount} 位成员 &middot; {data.firstMessage} ~ {data.lastMessage}</p>
      </div>
      <div className="stats-cards">
        <div className="stat-card"><div className="stat-value">{data.totalMessages.toLocaleString()}</div><div className="stat-label">总消息数</div></div>
        <div className="stat-card"><div className="stat-value">{data.memberCount}</div><div className="stat-label">成员数</div></div>
        <div className="stat-card"><div className="stat-value">{data.memberRanking[0]?.name || '-'}</div><div className="stat-label">最活跃成员</div></div>
        <div className="stat-card"><div className="stat-value">{data.topPhrases[0]?.phrase || '-'}</div><div className="stat-label">最常说的</div></div>
      </div>
      <div className="charts-grid">
        {hourlyOption && <div className="chart-card"><ReactECharts option={hourlyOption} style={{ height: 300 }} /></div>}
        {typeOption && <div className="chart-card"><ReactECharts option={typeOption} style={{ height: 300 }} /></div>}
        {weekdayOption && <div className="chart-card"><ReactECharts option={weekdayOption} style={{ height: 300 }} /></div>}
        {monthlyOption && <div className="chart-card"><ReactECharts option={monthlyOption} style={{ height: 300 }} /></div>}
      </div>
      <div className="ranking-section">
        <h3>消息排行 Top 20</h3>
        <div className="ranking-header"><span className="rank">#</span><span className="name">成员</span><span className="count">消息数</span><span className="bar"></span></div>
        {data.memberRanking.slice(0, 20).map((m, i) => (
          <div key={m.name} className="ranking-row">
            <span className="rank">{i + 1}</span>
            <span className="name">{m.name}</span>
            <span className="count">{m.messageCount.toLocaleString()}</span>
            <span className="bar"><span className="bar-fill" style={{ width: `${(m.messageCount / data.memberRanking[0].messageCount) * 100}%` }} /></span>
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

export default GroupAnalysisTab
