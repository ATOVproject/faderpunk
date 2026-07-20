import { H2, H3, H4, List } from "./Shared";

export const Troubleshooting = () => (
  <>
    <H2 id="troubleshooting">Troubleshooting</H2>

    <H3 id="connection-issues">Connection Issues</H3>
    <p>
      The Configurator connects to your Faderpunk over <strong>Web MIDI</strong>
      . If you're having trouble connecting, work through these checks in order:
    </p>

    <H4>1. Use a compatible browser</H4>
    <p>
      Web MIDI is supported in <strong>Chrome</strong>, <strong>Edge</strong>,{" "}
      <strong>Firefox</strong>, <strong>Brave</strong>, and{" "}
      <strong>Vivaldi</strong>.
    </p>
    <p>
      <strong>Safari does not support Web MIDI</strong> and cannot connect to a
      Faderpunk at all — if you're on a Mac, switch to one of the browsers
      above.
    </p>

    <H4>2. Grant MIDI access</H4>
    <p>
      The first time you connect, your browser shows a permission prompt asking
      to allow MIDI (SysEx) access — make sure to click <strong>Allow</strong>.
      If you clicked "Block" previously, open your browser's site settings for
      faderpunk.io, reset the MIDI permission, and reload the page.
    </p>

    <H4>3. Check your USB cable</H4>
    <p>
      Some USB cables only carry power, not data. Make sure you're using a cable
      that supports data transfer, not a charge-only cable.
    </p>

    <H4>4. Mac: clear stale MIDI ports after a firmware update</H4>
    <p>
      If you updated your Faderpunk's firmware from{" "}
      <strong>v1.10.x or earlier to v1.11.0 or later</strong>, macOS can leave
      old, stale "Faderpunk" entries behind in its MIDI configuration, which
      confuses the browser when it looks for your device. This only affects that
      update path — a fresh install of v1.11+ won't hit it. To fix it:
    </p>
    <ol className="mb-4 list-inside list-decimal space-y-2">
      <li>Unplug your Faderpunk</li>
      <li>
        Open <strong>Audio MIDI Setup</strong> (Applications → Utilities) →{" "}
        <strong>Window</strong> → <strong>Show MIDI Studio</strong>
      </li>
      <li>
        Delete every <strong>Faderpunk</strong> entry you find — including any
        greyed-out or duplicate ones
      </li>
      <li>Quit Audio MIDI Setup</li>
      <li>Replug your Faderpunk and try connecting again</li>
    </ol>

    <H4>5. Running older firmware?</H4>
    <p>
      Faderpunk devices on firmware older than v1.7 only support the previous
      USB connection method. The site detects this automatically: if no
      Faderpunk answers over MIDI, you'll be offered a{" "}
      <strong>"Connect via USB"</strong> fallback button — no manual setup
      needed.
    </p>

    <H4>Still stuck?</H4>
    <p>As a last resort, try a full reset of the connection:</p>
    <ol className="mb-4 list-inside list-decimal space-y-2">
      <li>Disconnect your Faderpunk from the USB port</li>
      <li>Close your browser completely</li>
      <li>Open your browser again</li>
      <li>
        Navigate to{" "}
        <a
          className="font-semibold underline"
          href="https://faderpunk.io"
          target="_blank"
          rel="noopener noreferrer"
        >
          faderpunk.io
        </a>
      </li>
      <li>
        Perform a hard refresh to clear the cache:
        <List>
          <li>
            <strong>Windows/Linux:</strong> Press <kbd>Ctrl</kbd> +{" "}
            <kbd>Shift</kbd> + <kbd>R</kbd> (or <kbd>Ctrl</kbd> + <kbd>F5</kbd>)
          </li>
          <li>
            <strong>Mac:</strong> Press <kbd>Cmd</kbd> + <kbd>Shift</kbd> +{" "}
            <kbd>R</kbd>
          </li>
        </List>
      </li>
      <li>Plug in your Faderpunk via USB</li>
      <li>Try connecting to Faderpunk again using the Connect button</li>
    </ol>

    <H3 id="factory-reset">Factory Reset</H3>
    <p>
      If your Faderpunk is experiencing issues or you want to restore it to its
      default settings, you can perform a hardware factory reset. This will:
    </p>
    <List>
      <li>Reset all app configurations to their defaults</li>
      <li>Clear all saved scenes</li>
      <li>Reset global settings (MIDI channels, I²C mode, etc.)</li>
      <li>
        <strong>Note:</strong> Calibration data will be preserved
      </li>
    </List>

    <p className="mt-4 font-bold">How to perform a factory reset:</p>
    <ol className="mb-4 list-inside list-decimal space-y-2">
      <li>
        Disconnect the USB cable from your Faderpunk (make sure it's completely
        powered off)
      </li>
      <li>
        <strong>Press and hold the first two channel buttons</strong> (the two
        leftmost buttons, Channel 1 and Channel 2)
      </li>
      <li>
        <strong>While holding both buttons</strong>, connect the USB cable to
        power on your Faderpunk
      </li>
      <li>
        <strong>Keep holding both buttons</strong> for about 2-3 seconds after
        the device powers on
      </li>
      <li>Release the buttons</li>
      <li>
        The device will perform the factory reset and automatically restart
      </li>
      <li>
        You'll see the bootup LED sequence as the device restarts with factory
        default settings
      </li>
      <li>
        Your Faderpunk is now ready to be reconfigured using the Configurator
      </li>
    </ol>

    <p className="mt-4">
      <strong>Important:</strong> This operation cannot be undone. Make sure to
      back up any layouts or settings you want to keep by exporting them from
      the Configurator before performing a factory reset.
    </p>
  </>
);
