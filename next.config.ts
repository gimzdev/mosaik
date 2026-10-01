import type { NextConfig } from 'next';

const security = [
  { key: 'X-Content-Type-Options', value: 'nosniff' },
  { key: 'Referrer-Policy', value: 'strict-origin-when-cross-origin' },
  { key: 'X-Frame-Options', value: 'DENY' },
  { key: 'Permissions-Policy', value: 'camera=(), microphone=(), geolocation=()' },
];

export default {
  poweredByHeader: false,
  agentRules: false, // don't drop AGENTS.md / CLAUDE.md into the site folder
  turbopack: { root: process.cwd() }, // this folder is the whole app, whatever sits around it
  headers: async () => [{ source: '/:path*', headers: security }],
} satisfies NextConfig;
