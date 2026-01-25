import { H2, H3, List } from "./Shared";

export const Troubleshooting = () => (
  <>
    <H2 id="troubleshooting">Troubleshooting</H2>

    <H3 id="connection-issues">Connection Issues</H3>
    <p>
      If you're having trouble connecting your Faderpunk to the Configurator,
      try these steps:
    </p>
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

    <p className="mt-4">
      <strong>Note:</strong> If you continue to experience connection issues,
      make sure you're using a browser that supports WebUSB (Chrome, Edge, or
      Opera) and that your USB cable supports data transfer (not just charging).
    </p>

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
