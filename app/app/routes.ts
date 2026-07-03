import { type RouteConfig, index, route } from '@react-router/dev/routes'

export default [
  index('routes/home.tsx'),
  route('docs', 'routes/docs.tsx'),
  route('flow', 'routes/flow.tsx'),
  route('trade', 'routes/trade/index.tsx'),
  route('trade/:asset', 'routes/trade/route.tsx'),
] satisfies RouteConfig
