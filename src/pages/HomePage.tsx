import { useNavigate } from 'react-router-dom'
import { KeyRound, Database, Download, ArrowRight, AlertTriangle } from 'lucide-react'
import { useAppStore } from '../stores/appStore'
import './HomePage.scss'

function HomePage() {
  const navigate = useNavigate()
  const { extractedKey, selectedDb } = useAppStore()

  const steps = [
    {
      icon: <KeyRound size={28} />,
      title: '提取密钥',
      desc: '自动从 QQ 进程中提取数据库加密密钥',
      done: !!extractedKey,
      path: '/key',
    },
    {
      icon: <Database size={28} />,
      title: '选择数据库',
      desc: '扫描并选择要导出的 QQ 聊天记录数据库',
      done: !!selectedDb,
      path: '/database',
    },
    {
      icon: <Download size={28} />,
      title: '导出记录',
      desc: '解密数据库并导出聊天记录为 TXT 文件',
      done: false,
      path: '/export',
    },
  ]

  return (
    <div className="home-page">
      <div className="home-content">
        <div className="home-hero">
          <h1 className="home-title">QQFlow</h1>
          <p className="home-subtitle">QQ 聊天记录本地解密导出工具</p>
          <p className="home-desc">所有数据仅在本地处理，绝不上传至任何服务器</p>
          <div className="home-risk-notice">
            <AlertTriangle size={14} />
            <span>本工具仅供个人学习、研究和数据备份用途。使用前请确保您有权访问相关数据，请遵守当地法律法规。</span>
          </div>
        </div>
        <div className="home-steps">
          {steps.map((step, i) => (
            <div
              key={i}
              className={`home-step-card ${step.done ? 'done' : ''}`}
              onClick={() => navigate(step.path)}
            >
              <div className="step-number">{step.done ? '✓' : i + 1}</div>
              <div className="step-icon">{step.icon}</div>
              <div className="step-info">
                <h3>{step.title}</h3>
                <p>{step.desc}</p>
              </div>
              <ArrowRight size={16} className="step-arrow" />
            </div>
          ))}
        </div>
      </div>
    </div>
  )
}

export default HomePage
