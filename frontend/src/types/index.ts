export interface RendererDevice {
  uuid: string
  name: string
  alias: string | null
  ip: string
  av_transport_url: string
  status: 'idle' | 'playing' | 'paused' | 'stopped' | 'error'
  current_media: string | null
  loop_playback: boolean
}

export interface Scene {
  name: string
  assignments: Record<string, string>
}

export interface SceneApplyResult {
  device_uuid: string
  success: boolean
  error: string | null
}

export interface AuthStatus {
  authenticated: boolean
  username: string | null
}

export interface MediaFileInfo {
  name: string
  size: number
  modified: number | null
  extension: string
}

export interface UploadMediaResponse {
  file: MediaFileInfo
}
