import { useEffect, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { exit } from '@tauri-apps/api/process';

function App() {
  const [ready, setReady] = useState(false);
  const [url, setUrl] = useState('');
  const [logs, setLogs] = useState<string[]>([]);
  const [showLogs, setShowLogs] = useState(false);

  useEffect(() => {
    const unlistenReady = listen<string>('server-ready', (e) => {
      setUrl(e.payload);
      setReady(true);
    });
    const unlistenLog = listen<string>('log', (e) => {
      setLogs((prev) => [...prev, e.payload]);
    });

    const handler = (e: KeyboardEvent) => {
      if (e.ctrlKey && e.key.toLowerCase() === 'r') {
        window.location.reload();
      } else if (e.ctrlKey && e.key.toLowerCase() === 'l') {
        e.preventDefault();
        setShowLogs((v) => !v);
      } else if (e.ctrlKey && e.key.toLowerCase() === 'q') {
        exit(0);
      }
    };
    window.addEventListener('keydown', handler);

    return () => {
      unlistenReady.then((f) => f());
      unlistenLog.then((f) => f());
      window.removeEventListener('keydown', handler);
    };
  }, []);

  if (!ready) {
    return (
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'center', height: '100vh' }}>
        Loading...
      </div>
    );
  }

  return (
    <div style={{ width: '100vw', height: '100vh' }}>
      <iframe src={url} style={{ border: 'none', width: '100%', height: '100%' }} />
      {showLogs && (
        <div
          style={{
            position: 'absolute',
            top: 0,
            left: 0,
            right: 0,
            bottom: 0,
            backgroundColor: 'rgba(0,0,0,0.8)',
            color: '#0f0',
            overflow: 'auto',
            padding: '1rem',
          }}
        >
          <pre>{logs.join('\n')}</pre>
        </div>
      )}
    </div>
  );
}

export default App;

