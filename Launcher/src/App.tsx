import { useEffect, useMemo, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { invoke } from '@tauri-apps/api/core';

const appWindow = getCurrentWindow();

type UpdateStatus = 'success' | 'upToDate' | 'needRetry' | 'failed';

interface UpdateResponse {
  status: UpdateStatus;
  message: string;
  logPath?: string;
  diff?: string;
  stashUsed: boolean;
}

interface CharacterResponse {
  success: boolean;
  message: string;
}

type Step =
  | 'updatePrompt'
  | 'updating'
  | 'updateRetry'
  | 'updateFailed'
  | 'stashPrompt'
  | 'characterPrompt'
  | 'characterRunning'
  | 'launching';

function App() {
  const [ready, setReady] = useState(false);
  const [url, setUrl] = useState('');
  const [logs, setLogs] = useState<string[]>([]);
  const [showLogs, setShowLogs] = useState(false);
  const [step, setStep] = useState<Step>('updatePrompt');
  const [updateResult, setUpdateResult] = useState<UpdateResponse | null>(null);
  const [characterResult, setCharacterResult] = useState<CharacterResponse | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [serverError, setServerError] = useState<string | null>(null);
  const [isProcessing, setIsProcessing] = useState(false);
  const [serverRequested, setServerRequested] = useState(false);

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
        e.preventDefault();
        void appWindow.close();
      }
    };
    window.addEventListener('keydown', handler);

    return () => {
      unlistenReady.then((f) => f());
      unlistenLog.then((f) => f());
      window.removeEventListener('keydown', handler);
    };
  }, []);

  useEffect(() => {
    if (step === 'launching' && !serverRequested) {
      setServerRequested(true);
      setServerError(null);
      void invoke('start_server')
        .catch((err) => {
          setServerError(err instanceof Error ? err.message : String(err));
          setServerRequested(false);
        });
    }
  }, [step, serverRequested]);

  const handleUpdate = async (attemptOverwrite: boolean) => {
    setError(null);
    setIsProcessing(true);
    setStep('updating');
    try {
      const result = await invoke<UpdateResponse>('update_vendor', { attemptOverwrite });
      setUpdateResult(result);
      if (result.status === 'success' || result.status === 'upToDate') {
        setStep(result.stashUsed ? 'stashPrompt' : 'characterPrompt');
      } else if (result.status === 'needRetry') {
        setStep('updateRetry');
      } else {
        setStep('updateFailed');
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      setStep('updateFailed');
    } finally {
      setIsProcessing(false);
    }
  };

  const handleSkipUpdate = () => {
    setUpdateResult(null);
    setStep('characterPrompt');
  };

  const handleRetryWithOverwrite = () => {
    void handleUpdate(true);
  };

  const handleSkipAfterFailure = () => {
    setUpdateResult((prev) =>
      prev ? { ...prev, message: 'WeylandTavern failed to update.' } : prev
    );
    setStep('updateFailed');
  };

  const handleStartAnyway = () => {
    setStep('characterPrompt');
  };

  const handleExit = () => {
    void appWindow.close();
  };

  const handleFinalizeStash = async (revert: boolean) => {
    setError(null);
    setIsProcessing(true);
    try {
      await invoke('finalize_stash', { revert });
      setStep('characterPrompt');
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setIsProcessing(false);
    }
  };

  const handleCharacterSync = async () => {
    setError(null);
    setCharacterResult(null);
    setIsProcessing(true);
    setStep('characterRunning');
    try {
      const result = await invoke<CharacterResponse>('run_character_sync');
      setCharacterResult(result);
    } catch (err) {
      setCharacterResult({
        success: false,
        message: err instanceof Error ? err.message : String(err),
      });
    } finally {
      setIsProcessing(false);
      setStep('launching');
    }
  };

  const handleSkipCharacter = () => {
    setCharacterResult({ success: false, message: 'Character update skipped.' });
    setStep('launching');
  };

  const retryServer = () => {
    setServerError(null);
    setServerRequested(false);
  };

  const buttonRowStyle = useMemo(
    () => ({ display: 'flex', gap: '0.75rem', marginTop: '1rem', flexWrap: 'wrap' as const }),
    []
  );

  const renderUpdateDetails = () => {
    if (!updateResult || !updateResult.logPath) {
      return null;
    }
    return (
      <div style={{ marginTop: '0.75rem', maxWidth: '42rem' }}>
        <p>
          A log file was written to <code>{updateResult.logPath}</code>.
        </p>
        {updateResult.diff && (
          <pre
            style={{
              backgroundColor: '#111',
              color: '#0f0',
              padding: '0.75rem',
              borderRadius: '0.5rem',
              maxHeight: '12rem',
              overflow: 'auto',
            }}
          >
            {updateResult.diff}
          </pre>
        )}
      </div>
    );
  };

  const renderStepContent = () => {
    switch (step) {
      case 'updatePrompt':
        return (
          <>
            <p>Do you want to check for WeylandTavern updates?</p>
            <div style={buttonRowStyle}>
              <button onClick={() => handleUpdate(false)} disabled={isProcessing}>
                Yes
              </button>
              <button onClick={handleSkipUpdate} disabled={isProcessing}>
                No
              </button>
            </div>
          </>
        );
      case 'updating':
        return <p>Updating WeylandTavern...</p>;
      case 'updateRetry':
        return (
          <>
            <p>There was an error updating WeylandTavern.</p>
            {renderUpdateDetails()}
            <div style={buttonRowStyle}>
              <button onClick={handleRetryWithOverwrite} disabled={isProcessing}>
                Retry with overwrite
              </button>
              <button onClick={handleSkipAfterFailure} disabled={isProcessing}>
                Skip update
              </button>
              <button onClick={handleExit} disabled={isProcessing}>
                Exit
              </button>
            </div>
          </>
        );
      case 'updateFailed':
        return (
          <>
            <p>{updateResult?.message ?? 'WeylandTavern failed to update.'}</p>
            <p>Start WeylandTavern anyway?</p>
            {renderUpdateDetails()}
            <div style={buttonRowStyle}>
              <button onClick={handleStartAnyway} disabled={isProcessing}>
                Start anyway
              </button>
              <button onClick={handleExit} disabled={isProcessing}>
                Exit
              </button>
            </div>
          </>
        );
      case 'stashPrompt':
        return (
          <>
            <p>Revert differing files post update?</p>
            <div style={buttonRowStyle}>
              <button onClick={() => void handleFinalizeStash(true)} disabled={isProcessing}>
                Yes
              </button>
              <button onClick={() => void handleFinalizeStash(false)} disabled={isProcessing}>
                No
              </button>
            </div>
          </>
        );
      case 'characterPrompt':
        return (
          <>
            <p>Do you want to check for character updates?</p>
            <div style={buttonRowStyle}>
              <button onClick={() => void handleCharacterSync()} disabled={isProcessing}>
                Yes
              </button>
              <button onClick={handleSkipCharacter} disabled={isProcessing}>
                No
              </button>
            </div>
          </>
        );
      case 'characterRunning':
        return <p>Checking for character updates...</p>;
      case 'launching':
        return (
          <>
            <p>Starting WeylandTavern...</p>
            {characterResult && (
              <p style={{ marginTop: '0.5rem' }}>{characterResult.message}</p>
            )}
            {serverError && (
              <div style={{ marginTop: '1rem' }}>
                <p style={{ color: '#ff8a80' }}>Failed to start the server: {serverError}</p>
                <button onClick={retryServer}>Retry start</button>
              </div>
            )}
          </>
        );
      default:
        return null;
    }
  };

  if (ready) {
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

  return (
    <div
      style={{
        display: 'flex',
        flexDirection: 'column',
        alignItems: 'center',
        justifyContent: 'center',
        minHeight: '100vh',
        padding: '2rem',
        gap: '1rem',
        textAlign: 'center',
      }}
    >
      <h1>WeylandTavern Launcher</h1>
      {updateResult && (updateResult.status === 'success' || updateResult.status === 'upToDate') && (
        <p>{updateResult.message}</p>
      )}
      {renderStepContent()}
      {error && <p style={{ color: '#ff8a80' }}>{error}</p>}
      <p style={{ fontSize: '0.9rem', opacity: 0.75 }}>
        Use <kbd>Ctrl</kbd>+<kbd>L</kbd> to toggle logs, <kbd>Ctrl</kbd>+<kbd>R</kbd> to reload, and <kbd>Ctrl</kbd>+<kbd>Q</kbd> to quit.
      </p>
      {showLogs && (
        <div
          style={{
            position: 'fixed',
            top: 0,
            left: 0,
            right: 0,
            bottom: 0,
            backgroundColor: 'rgba(0,0,0,0.85)',
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
