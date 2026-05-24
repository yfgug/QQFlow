export interface Database {
  qq: string
  path: string
  size_mb: number
}

export interface LogMessage {
  level: string
  text: string
}

export interface ExportSummary {
  total: number
  groups: number
  private: number
  dir: string
}

export interface SimpleResponse {
  ok: boolean
  error?: string
}
