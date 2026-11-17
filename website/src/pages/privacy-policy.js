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
Platform providers and underlying operating systems (android, ios, web browsers, app store) may collect information about the application (crash reports, usage stats) and make those available to Fastxt App developer, who may use those data to improve the application.
</li>
</ul>

<h2>Dictation</h2>
<ul>
<li>
Dictation recognizes speech on your iPhone with one of two recognizers: the one built into iOS, set to recognize on the device only, or SenseVoice Small, a speech model inside the app. Fastxt never sends your voice to a server to be recognized: when neither can recognize a language on the phone, dictation doesn't start.
</li>
<li>
The audio is never stored or sent anywhere. Only the text you save is kept, as a note.
</li>
<li>
iOS asks for your permission to use the microphone before the first dictation, and to use its speech recognition before Fastxt first uses it.
</li>
</ul>

<h2>Coding Agents</h2>
<ul>
<li>
When you send a note to a coding-agent session, its text goes from your phone to your paired computer over the same encrypted connection as sync, and Fastxt types it into the session you picked.
</li>
<li>
If you add Fastxt's MCP server to an agent, that agent can search, read, add, change and delete your notes through it, and take the prompts you prepared. The server runs on your own computer.
</li>
<li>
From there, the agent treats the text like anything you type into it. An agent that uses a cloud model sends it to that model's provider, under the provider's terms. Fastxt has no part in that, and no server of its own.
</li>
<li>
Each send or take is recorded with its note: when, which agent, which session and which project folder. The record stays on your devices and syncs with the note; deleting the note deletes its records on every device.
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
All of this data is stored in your local SQLite database alongside your notes and is never transmitted to external services, unless you give an agent access to your notes (see Coding Agents above).
</p>

</div>

      </div>
    </Layout>
  );
}

export default Hello;
