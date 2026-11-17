import React from 'react';
import clsx from 'clsx';
import Layout from '@theme/Layout';
import Link from '@docusaurus/Link';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';
import useBaseUrl from '@docusaurus/useBaseUrl';
import styles from './styles.module.css';

const steps = [
  {
    title: 'Dictate on your phone',
    description: (
      <>
        Speak an idea or a prompt, in English, Chinese, or both in one
        sentence. The iPhone recognizes it on the device itself, with iOS or
        the speech model inside Fastxt: the audio never leaves the phone and
        is never stored. Correct it by typing, and keep it as an idea or a
        prompt.
      </>
    ),
  },
  {
    title: 'Send it to a session',
    description: (
      <>
        Pair the phone with your computer and pick one of the coding-agent
        sessions you have open there in Orca: Claude Code, Codex and the others
        it runs. Fastxt types the prompt in, as if you had, and never into a
        session that is asking you a question.
      </>
    ),
  },
  {
    title: 'Or let the agent take it',
    description: (
      <>
        Any agent with Fastxt's MCP server can take your newest prompt when you
        ask; in Claude Code, that's <code>/mcp__fastxt__take</code>. Every
        device records which agent had which prompt, and in which project.
      </>
    ),
  },
];

const features = [
  {
    title: <>No Server in Between</>,
    imageUrl: 'img/fastxt-sync-server-qr-mac.png',
    description: (
      <>
        Your notes live in a SQLite database on each of your devices and sync
        directly between them, over an encrypted connection pinned by a
        pairing code. No account, no Fastxt server.
      </>
    ),
  },
  {
    title: <>On-Device AI</>,
    imageUrl: 'img/undraw_fast_loading_0lbh.svg',
    description: (
      <>
        Tags and summaries on the phone, from Apple's on-device models; semantic
        search and categories too on the desktop, from a model server on your
        own machine, such as Ollama.
      </>
    ),
  },
  {
    title: <>Cross Platform</>,
    imageUrl: 'img/undraw_mobile_devices_k1ok.svg',
    description: (
      <>
        iPhone and iPad, Android, macOS, Windows and Linux, plus one{' '}
        <code>fastxt</code> command for your computer. Devices on the same
        network pair by scanning a QR code.
      </>
    ),
  },
];

const roadmapItems = [
  {
    title: 'Android',
    description: 'Dictation and sending to coding agents on Android, with the same speech model.',
  },
  {
    title: 'Semantic Search on the Phone',
    description:
      'One multilingual embedding model on every device, so the phones search by meaning too.',
  },
];

function Feature({imageUrl, title, description}) {
  const imgUrl = useBaseUrl(imageUrl);
  return (
    <div className={clsx('col col--4', styles.feature)}>
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
      description="Dictate ideas and prompts on your phone and hand them to your coding agents. Local-first notes with on-device AI.">
      <header className={clsx('hero hero--primary', styles.heroBanner)}>
        <div className="container">
          <h1 className="hero__title">{siteConfig.title}</h1>
          <p className="hero__subtitle">{siteConfig.tagline}</p>
          <div className={styles.buttons}>
            <Link
              className={clsx(
                'button button--outline button--secondary button--lg',
                styles.getStarted,
              )}
              to="https://apps.apple.com/us/app/fastxt/id1488406732">
              iOS & iPadOS
            </Link>
            <Link
              className={clsx(
                'button button--outline button--secondary button--lg',
                styles.getStarted,
              )}
              to="https://apps.apple.com/us/app/fastxt-desktop/id1508900852">
              macOS
            </Link>
          </div>
          <p className={styles.status}>
            New in the source code, not yet in the App Store apps: dictation,
            and handing your prompts to coding agents.{' '}
            <Link to="/docs/agents">How it works</Link>
          </p>
        <div className="container">
            <iframe width="560" height="315" src="https://www.youtube-nocookie.com/embed/fB-XNO6-TXo" title="YouTube video player" frameborder="0" allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture" allowfullscreen></iframe>
        </div>
        </div>
      </header>
      <main>
        <section className={styles.features}>
          <div className="container">
            <h2 style={{textAlign: 'center', marginBottom: '2rem'}}>From Your Voice to Your Coding Agent</h2>
            <div className="row">
              {steps.map((item, idx) => (
                <div key={idx} className={clsx('col col--4', styles.feature)}>
                  <h3>{idx + 1}. {item.title}</h3>
                  <p>{item.description}</p>
                </div>
              ))}
            </div>
          </div>
        </section>
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
                  <div key={idx} className={clsx('col col--4', styles.feature)}>
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
