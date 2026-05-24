/// Global application state using React Context.
/// Replaces Zustand store from the original Electron version.

import { createContext, useContext, useState, useCallback, ReactNode } from 'react'
import type { Database, LogMessage, ExportSummary } from '../lib/types'

interface AppState {
  // Key management (multi-QQ support)
  savedKeys: Record<string, string>
  setSavedKeys: (keys: Record<string, string>) => void
  extractedKey: string | null // derived: key for selectedDb.qq
  keyExtracting: boolean
  setKeyExtracting: (v: boolean) => void
  keyLogs: LogMessage[]
  addKeyLog: (msg: LogMessage) => void
  clearKeyLogs: () => void

  // Database
  databases: Database[]
  setDatabases: (dbs: Database[]) => void
  selectedDb: Database | null
  setSelectedDb: (db: Database | null) => void

  // Export
  exporting: boolean
  setExporting: (v: boolean) => void
  exportDone: boolean
  setExportDone: (v: boolean) => void
  exportLogs: LogMessage[]
  addExportLog: (msg: LogMessage) => void
  addExportLogs: (msgs: LogMessage[]) => void
  clearExportLogs: () => void
  exportSummary: ExportSummary | null
  setExportSummary: (s: ExportSummary | null) => void
}

const AppContext = createContext<AppState | null>(null)

const MAX_LOGS = 300

export function AppProvider({ children }: { children: ReactNode }) {
  const [savedKeys, setSavedKeys] = useState<Record<string, string>>({})
  const [keyExtracting, setKeyExtracting] = useState(false)
  const [keyLogs, setKeyLogs] = useState<LogMessage[]>([])
  const [databases, setDatabases] = useState<Database[]>([])
  const [selectedDb, setSelectedDb] = useState<Database | null>(null)

  // Derive current key from selected database
  const extractedKey = selectedDb ? (savedKeys[selectedDb.qq] || null) : null
  const [exporting, setExporting] = useState(false)
  const [exportDone, setExportDone] = useState(false)
  const [exportLogs, setExportLogs] = useState<LogMessage[]>([])
  const [exportSummary, setExportSummary] = useState<ExportSummary | null>(null)

  const addKeyLog = useCallback((msg: LogMessage) => {
    setKeyLogs((prev) => {
      const next = [...prev, msg]
      return next.length > MAX_LOGS ? next.slice(-MAX_LOGS) : next
    })
  }, [])

  const clearKeyLogs = useCallback(() => setKeyLogs([]), [])

  const addExportLog = useCallback((msg: LogMessage) => {
    setExportLogs((prev) => {
      const next = [...prev, msg]
      return next.length > MAX_LOGS ? next.slice(-MAX_LOGS) : next
    })
  }, [])

  const addExportLogs = useCallback((msgs: LogMessage[]) => {
    setExportLogs((prev) => {
      const next = [...prev, ...msgs]
      return next.length > MAX_LOGS ? next.slice(-MAX_LOGS) : next
    })
  }, [])

  const clearExportLogs = useCallback(() => setExportLogs([]), [])

  return (
    <AppContext.Provider
      value={{
        savedKeys,
        setSavedKeys,
        extractedKey,
        keyExtracting,
        setKeyExtracting,
        keyLogs,
        addKeyLog,
        clearKeyLogs,
        databases,
        setDatabases,
        selectedDb,
        setSelectedDb,
        exporting,
        setExporting,
        exportDone,
        setExportDone,
        exportLogs,
        addExportLog,
        addExportLogs,
        clearExportLogs,
        exportSummary,
        setExportSummary,
      }}
    >
      {children}
    </AppContext.Provider>
  )
}

export function useAppStore() {
  const ctx = useContext(AppContext)
  if (!ctx) throw new Error('useAppStore must be used within AppProvider')
  return ctx
}
