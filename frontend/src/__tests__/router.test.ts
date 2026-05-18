import { describe, expect, it } from 'vitest'
import router from '@/router'

describe('router', () => {
  it('exposes the about route', () => {
    const routePaths = router.getRoutes().map(route => route.path)

    expect(routePaths).toContain('/about')
    expect(routePaths).toContain('/devices')
    expect(routePaths).toContain('/media')
    expect(routePaths).toContain('/scenes')
  })
})
