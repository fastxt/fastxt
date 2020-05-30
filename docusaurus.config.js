module.exports = {
  title: 'Fastxt',
  tagline: 'Own your txt on your device.',
  url: 'https://fastxt.app',
  baseUrl: '/',
  favicon: 'img/favicon.ico',
  organizationName: 'fastxt', // Usually your GitHub org/user name.
  projectName: 'fastxt', // Usually your repo name.
  themeConfig: {
    navbar: {
      title: 'Fastxt',
      logo: {
        alt: 'Fastxt',
        src: 'img/logo.png',
      },
      links: [
        {
          to: 'docs/developer-setup',
          activeBasePath: 'docs',
          label: 'Docs',
          position: 'left',
        },
        {to: 'blog', label: 'Blog', position: 'left'},
        {to: 'privacy-policy', label: 'Privacy', position: 'left'},
        {
          href: 'https://gitlab.com/fastxt/fastxt',
          label: 'GitLab',
          position: 'right',
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
              label: 'Privacy Policy',
              to: 'privacy-policy',
            },
          ],
        },
        {
          title: 'Video',
          items: [
            {
              label: 'Youtube',
              href: 'https://www.youtube.com/channel/UCO3qFIyK0eSmqvMknsslWRw',
            },
          ],
        },
        {
          title: 'Code',
          items: [
            {
              label: 'GitLab',
              href: 'https://gitlab.com/fastxt',
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
          editUrl:
            'https://gitlab.com/fastxt/fastxt-app-docusaurus/edit/master/',
        },
        theme: {
          customCss: require.resolve('./src/css/custom.css'),
        },
      },
    ],
  ],
};
