import React from 'react';
import Layout from '@theme/Layout';

function Hello() {
  return (
    <Layout title="Privacy Policy">
      <div
        style={{
          display: 'flex',
          justifyContent: 'center',
          alignItems: 'center',
          minHeight: '50vh',
          fontSize: '20px',
          padding: '2rem',
        }}>

        <div className="post">
  <header className="postHeader">
    <h1>Privacy Policy</h1>
  </header>
<ul>
<li>
Fastxt App stores your information locally on your own device.
</li>
<li>
Fastxt App developer does not run any service to collect or store any personal information about you.
</li>
<li>
If you choose to sync data via LAN/WiFi to other devices, be aware that is you voluntarily share your data with other peers.
</li>
<li>
Platform providers and underlying operating systems (android, ios, web browsers, app store) may collect information about the application (crash reports, usage stats) and make those avaliable to Fastxt App developer, who may use those data to improve the application.
</li>
</ul>

<h2>On-Device AI Privacy</h2>
<p>
Fastxt includes AI-powered features (smart tagging, summarization, semantic search, and organization) that process your notes locally on your device. Here's how we protect your privacy:
</p>
<ul>
<li>
<strong>All AI processing happens on your device.</strong> Your notes are never sent to cloud servers for AI analysis. The AI models run entirely on your hardware.
</li>
<li>
<strong>Desktop (Ollama):</strong> When using Ollama for AI features, the language model runs locally on your computer. No data is transmitted to external servers. You have full control over which model to use and where it runs.
</li>
<li>
<strong>iOS/macOS (Apple Foundation Models):</strong> AI features use Apple's on-device Neural Engine. Apple's privacy protections apply — your data is processed on-device and not uploaded to Apple's servers.
</li>
<li>
<strong>Android:</strong> AI features use on-device ML models provided by the Android platform. All processing happens locally on your device.
</li>
<li>
<strong>No training on your data:</strong> We do not use your notes to train AI models. The models are pre-trained and used only for inference on your device.
</li>
<li>
<strong>AI-generated metadata syncs with your notes:</strong> When you sync notes between devices, AI-generated tags, summaries, and embeddings are included. This data is treated the same as your note content — stored locally and synced peer-to-peer.
</li>
</ul>

<h3>What Data Does AI Process?</h3>
<p>
The AI features analyze your note text to generate:
</p>
<ul>
<li>Suggested tags based on content</li>
<li>Summaries of longer notes</li>
<li>Embeddings (numerical representations) for semantic search</li>
<li>Category suggestions for organization</li>
</ul>
<p>
All of this data is stored in your local SQLite database alongside your notes and is never transmitted to external services.
</p>

</div>

      </div>
    </Layout>
  );
}

export default Hello;
