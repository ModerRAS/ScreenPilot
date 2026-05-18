import { describe, expect, it } from 'vitest'
import { formatBuildDate, formatDisplayVersion } from '@/version'

describe('version helpers', () => {
  it('formats nightly builds with build number and commit', () => {
    expect(
      formatDisplayVersion({
        version: '0.1.0',
        channel: 'nightly',
        buildNumber: '42',
        gitShortSha: 'abc1234',
      }),
    ).toBe('v0.1.0 nightly.42 abc1234')
  })

  it('keeps non-nightly channel labels simple', () => {
    expect(
      formatDisplayVersion({
        version: '0.1.0',
        channel: 'dev',
        buildNumber: '',
        gitShortSha: '',
      }),
    ).toBe('v0.1.0 dev')
  })

  it('formats build dates and tolerates invalid values', () => {
    expect(formatBuildDate('')).toBe('Unknown')
    expect(formatBuildDate('not-a-date')).toBe('not-a-date')
    expect(formatBuildDate('2026-05-18T00:00:00.000Z')).not.toBe('Unknown')
  })
})
