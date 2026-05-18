interface BuildInfo {
  appName: string
  version: string
  channel: string
  buildNumber: string
  buildDate: string
  gitSha: string
  gitShortSha: string
  target: string
}

declare const __SCREENPILOT_VERSION__: string
declare const __SCREENPILOT_BUILD_CHANNEL__: string
declare const __SCREENPILOT_BUILD_NUMBER__: string
declare const __SCREENPILOT_BUILD_DATE__: string
declare const __SCREENPILOT_GIT_SHA__: string
declare const __SCREENPILOT_GIT_SHORT_SHA__: string
declare const __SCREENPILOT_TARGET__: string

export const buildInfo: BuildInfo = {
  appName: 'ScreenPilot',
  version: __SCREENPILOT_VERSION__,
  channel: __SCREENPILOT_BUILD_CHANNEL__,
  buildNumber: __SCREENPILOT_BUILD_NUMBER__,
  buildDate: __SCREENPILOT_BUILD_DATE__,
  gitSha: __SCREENPILOT_GIT_SHA__,
  gitShortSha: __SCREENPILOT_GIT_SHORT_SHA__,
  target: __SCREENPILOT_TARGET__,
}

export function formatDisplayVersion(info: Pick<BuildInfo, 'version' | 'channel' | 'buildNumber' | 'gitShortSha'>): string {
  return [
    `v${info.version}`,
    info.channel === 'nightly' && info.buildNumber ? `nightly.${info.buildNumber}` : info.channel,
    info.gitShortSha,
  ].filter(Boolean).join(' ')
}

export const displayVersion = formatDisplayVersion(buildInfo)

export function formatBuildDate(value: string): string {
  if (!value) return 'Unknown'
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return value
  return date.toLocaleString()
}
