import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';
import starlightLinksValidator from 'starlight-links-validator';

export default defineConfig({
  site: 'https://sergio-sisternes-epam.github.io',
  base: '/aslm',
  integrations: [
    starlight({
      title: 'SML — Skill Markup Language',
      social: {
        github: 'https://github.com/sergio-sisternes-epam/aslm',
      },
      sidebar: [
        {
          label: 'Language Specification',
          autogenerate: { directory: 'specification' },
        },
        {
          label: 'Architecture Decisions',
          autogenerate: { directory: 'adrs' },
        },
        {
          label: 'User Guide',
          autogenerate: { directory: 'guide' },
        },
        {
          label: 'API Reference',
          autogenerate: { directory: 'api' },
        },
        {
          label: 'Installation',
          autogenerate: { directory: 'installation' },
        },
      ],
      plugins: [starlightLinksValidator()],
      customCss: ['./src/styles/custom.css'],
    }),
  ],
});
