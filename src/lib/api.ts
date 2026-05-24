/// Tauri API wrapper.
/// Replaces window.electronAPI with Tauri invoke calls.

import { invoke } from '@tauri-apps/api/core'
import type { Database, LogMessage, ExportSummary, SimpleResponse } from './types'

export const api = {
  ping: () => invoke<{ ok: boolean }>('ping'),

  scanDatabases: () =>
    invoke<{ ok: boolean; databases: Database[]; error?: string }>('scan_databases'),

  extractKey: () => invoke<SimpleResponse>('extract_key'),

  getKeyStatus: () =>
    invoke<{ done: boolean; ok: boolean; key?: string; messages: LogMessage[] }>('get_key_status'),

  cancelExport: () => invoke<SimpleResponse>('cancel_export'),

  clearMsgStore: () => invoke<SimpleResponse>('clear_msg_store'),

  startExport: (params: { db_path: string; key: string; output_dir?: string; group_ids?: string[]; peer_ids?: string[] }) =>
    invoke<SimpleResponse>('start_export', { params }),

  getExportStatus: () =>
    invoke<{
      done: boolean
      ok: boolean
      messages: LogMessage[]
      summary?: ExportSummary
    }>('get_export_status'),

  exportCsv: (params: { db_path: string; key: string; output_dir?: string; group_ids?: string[]; peer_ids?: string[] }) =>
    invoke<{ ok: boolean; groups?: number; private?: number; total?: number; dir?: string; error?: string }>('export_csv', { params }),

  getCsvProgress: () =>
    invoke<{ done: boolean; messages: LogMessage[] }>('get_csv_progress'),

  getAnalysisProgress: () =>
    invoke<{ done: boolean; messages: LogMessage[] }>('get_analysis_progress'),

  analyzeGroup: (params: { db_path: string; key: string; group_id?: string }) =>
    invoke<{ ok: boolean; data?: any; error?: string }>('analyze_group', { params }),

  analyzePrivate: (params: { db_path: string; key: string; peer_id?: string }) =>
    invoke<{ ok: boolean; data?: any; error?: string }>('analyze_private', { params }),

  debugDbSchema: (params: { db_path: string; key: string }) =>
    invoke<{ ok: boolean; data?: any; error?: string }>('debug_db_schema', { params }),

  saveKey: (key: string, qqNumber: string) => invoke<SimpleResponse>('save_key', { key, qqNumber }),

  loadKeys: () => invoke<{ ok: boolean; keys?: Record<string, string> }>('load_keys'),

  clearKey: (qqNumber: string) => invoke<SimpleResponse>('clear_key', { qqNumber }),
}
