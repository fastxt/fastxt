import React from 'react';
import classnames from 'classnames';
import Layout from '@theme/Layout';
import Link from '@docusaurus/Link';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';
import useBaseUrl from '@docusaurus/useBaseUrl';
import styles from './styles.module.css';

const features = [
  {
    title: <>On-Device AI</>,
    imageUrl: 'img/undraw_fast_loading_0lbh.svg',
    description: (
      <>
        Native AI models on every platform. <br/>
        Apple Foundation Models, Android App Functions, self-hosted LLMs. <br/>
        Your data never leaves your device.
      </>
    ),
  },
  {
    title: <>Decentralized</>,
    imageUrl: 'img/fastxt-sync-server-qr-mac.png',
    description: (
      <>
      Fastxt empowers you to save and sync your text in local SQLite database without going through any centralized service.
      </>
    ),
  },
  {
    title: <>Cross Platform</>,
    imageUrl: 'img/undraw_mobile_devices_k1ok.svg',
    description: (
      <>
      Sync between multiple devices you own (desktop or mobile) by scanning QR code generated on the same network (Wi-Fi/LAN).
      </>
    ),
  },
];

const roadmapItems = [
  {
    title: 'Smart Tagging',
    description: 'Auto-suggest tags for saved text based on content analysis using on-device AI.',
  },
  {
    title: 'Semantic Search',
    description: 'Find notes by meaning, not just keyword matching — powered by local embeddings.',
  },
  {
    title: 'Summarization',
    description: 'Condense long text entries into concise summaries, all processed on-device.',
  },
  {
    title: 'AI-Assisted Organization',
    description: 'Automatic grouping and categorization of notes using native AI models.',
  },
  {
    title: 'Cross-Device AI Sync',
    description: 'AI-generated metadata (tags, summaries) syncs alongside your text via P2P.',
  },
];

function Feature({imageUrl, title, description}) {
  const imgUrl = useBaseUrl(imageUrl);
  return (
    <div className={classnames('col col--4', styles.feature)}>
      {imgUrl && (
        <div className="text--center">
          <img className={styles.featureImage2} src={imgUrl} alt={title} />
        </div>
      )}
      <h3>{title}</h3>
      <p>{description}</p>
    </div>
  );
}

function Home() {
  const context = useDocusaurusContext();
  const {siteConfig = {}} = context;
  return (
    <Layout
      title={`${siteConfig.title}`}
      description="Decentralized cross-platform text app with on-device AI">
      <header className={classnames('hero hero--primary', styles.heroBanner)}>
        <div className="container">
          <h1 className="hero__title">{siteConfig.title}</h1>
          <p className="hero__subtitle">{siteConfig.tagline}</p>
          <div className={styles.buttons}>
            <Link
              className={classnames(
                'button button--outline button--secondary button--lg',
                styles.getStarted,
              )}
              to="https://apps.apple.com/us/app/fastxt/id1488406732">
              iOS & iPadOS
            </Link>
            <Link
              className={classnames(
                'button button--outline button--secondary button--lg',
                styles.getStarted,
              )}
              to="https://apps.apple.com/us/app/fastxt-desktop/id1508900852">
              macOS
            </Link>
          </div>
        <div className="container">
            <iframe width="560" height="315" src="https://www.youtube-nocookie.com/embed/fB-XNO6-TXo" title="YouTube video player" frameborder="0" allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture" allowfullscreen></iframe>
        </div>
        </div>
      </header>
      <main>
        {features && features.length && (
          <section className={styles.features}>
            <div className="container">
              <div className="row">
                {features.map((props, idx) => (
                  <Feature key={idx} {...props} />
                ))}
              </div>
            </div>
          </section>
        )}
        {roadmapItems && roadmapItems.length && (
          <section className={styles.features}>
            <div className="container">
              <h2 style={{textAlign: 'center', marginBottom: '2rem'}}>Roadmap</h2>
              <div className="row">
                {roadmapItems.map((item, idx) => (
                  <div key={idx} className={classnames('col col--4', styles.feature)}>
                    <h3>{item.title}</h3>
                    <p>{item.description}</p>
                  </div>
                ))}
              </div>
            </div>
          </section>
        )}
      </main>
      <div class="container">
        <div><h3> Scan with WeChat </h3> </div>
        <img src="img/wechat/localnative-wechat-qrcode_344.jpg" width="150" />
        <div> Keep up to date with news and tutorial. </div>
      </div>
    </Layout>
  );
}

export default Home;
