module.exports = {
  title: 'Fastxt',
  tagline: 'Own your txt. On-device AI. Sync everywhere.',
  url: 'https://fastxt.app',
  baseUrl: '/',
  favicon: 'img/favicon.ico',
  organizationName: 'fastxt',
  projectName: 'fastxt',
  themeConfig: {
    colorMode: {
      defaultMode: 'dark',
      disableSwitch: true,
      respectPrefersColorScheme: false,
    },
    navbar: {
      title: 'Fastxt',
      logo: {
        alt: 'Fastxt',
        src: 'img/logo.png',
      },
      items: [
        {
          to: 'docs/developer-setup',
          activeBasePath: 'docs',
          label: 'Docs',
          position: 'left',
        },
        {
          to: 'docs/ai-features',
          label: 'AI Features',
          position: 'left',
        },
        {to: 'blog', label: 'Blog', position: 'left'},
        {to: 'privacy-policy', label: 'Privacy', position: 'left'},
        {
          href: 'https://github.com/fastxt/fastxt',
          position: 'right',
          className: 'header-github-link',
          'aria-label': 'GitHub repository',
        },
      ],
    },
    footer: {
      style: 'dark',
      links: [
        {
          title: 'Docs',
          items: [
            {
              label: 'Developer Setup',
              to: 'docs/developer-setup',
            },
            {
              label: 'AI Features',
              to: 'docs/ai-features',
            },
            {
              label: 'Privacy Policy',
              to: 'privacy-policy',
            },
          ],
        },
        {
          title: 'Community',
          items: [
            {
              label: 'Librem Social',
              href: 'https://social.librem.one/@yi',
            },
            {
              label: 'Open Collective',
              href: 'https://opencollective.com/fastxt',
            },
          ],
        },
        {
          title: 'Video',
          items: [
            {
              label: 'Youtube',
              href: 'https://www.youtube.com/channel/UCJemcWCEswRWrfHV7SlAgrw',
            },
          ],
        },
        {
          title: 'Code',
          items: [
            {
              label: 'GitHub',
              href: 'https://github.com/fastxt',
            }
          ],
        },
      ],
      copyright: `Unless otherwise noted, contents on this website are copyleft with a CC-by-SA 4.0 license.`,
    },
  },
  presets: [
    [
      '@docusaurus/preset-classic',
      {
        docs: {
          sidebarPath: require.resolve('./sidebars.js'),
          editUrl: 'https://github.com/fastxt/fastxt/tree/main/website/',
        },
        theme: {
          customCss: require.resolve('./src/css/custom.css'),
        },
      },
    ],
  ],
};
