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
If you choose to sync data between devices, notes travel directly between your devices over an authenticated, TLS-encrypted connection (the pairing code carries a certificate fingerprint and a one-time session token). No Fastxt server relays or stores your data. Be aware that syncing shares your data with the devices you pair with.
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
<strong>Desktop:</strong> AI features use a local LLM backend (Ollama by default, or any OpenAI-compatible server such as llama.cpp). The default endpoint is your own machine; if you point the endpoint at another machine, your note text is sent to that machine for inference — you are in control of where it runs.
</li>
<li>
<strong>iOS/macOS (Apple Foundation Models and Natural Language):</strong> AI features use Apple's on-device frameworks. Apple's privacy protections apply — your data is processed on-device and not uploaded to Apple's servers.
</li>
<li>
<strong>Android:</strong> AI features use on-device heuristics. All processing happens locally on your device.
</li>
<li>
<strong>No training on your data:</strong> We do not use your notes to train AI models. The models are pre-trained and used only for inference on your device.
</li>
<li>
<strong>AI-generated metadata syncs with your notes:</strong> When you sync notes between devices, AI-generated tags, summaries, and embeddings travel along with them over the same encrypted channel.
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
