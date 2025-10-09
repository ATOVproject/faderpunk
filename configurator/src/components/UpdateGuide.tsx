export const UpdateGuide = () => (
  <>
    <h2 className="text-yellow-fp mt-8 mb-4 text-lg font-bold uppercase">
      Faderpunk Firmware Update Guide
    </h2>
    <p>
      This guide will walk you through updating your Faderpunk's firmware. Don't
      worry—it's simpler than it sounds! You'll essentially be copying a file to
      your device while it's in a special update mode.
    </p>

    <h3 className="mt-6 font-bold">What You'll Need</h3>
    <ul className="mb-4 list-inside list-disc">
      <li>Your Faderpunk synth controller</li>
      <li>A USB cable to connect it to your computer</li>
      <li>
        The firmware file (it will end in <code>.uf2</code>)
      </li>
    </ul>

    <h3 className="mt-6 font-bold">Step 1: Enter Bootloader Mode</h3>
    <p className="mb-2">
      This is a special mode that allows your Faderpunk to receive new firmware.
    </p>
    <ol className="mb-4 list-inside list-decimal space-y-1">
      <li>
        <strong>Before plugging in the USB cable</strong>, locate the{" "}
        <strong>Shift button</strong> on your Faderpunk (it's the bottom-right
        button on the unit)
      </li>
      <li>
        <strong>Press and hold the Shift button</strong>
      </li>
      <li>
        <strong>While still holding Shift</strong>, connect the USB cable from
        your Faderpunk to your computer
      </li>
      <li>
        <strong>Keep holding Shift</strong> for about 2-3 seconds after plugging
        in
      </li>
      <li>You can now release the Shift button</li>
    </ol>

    <h3 className="mt-6 font-bold">Step 2: Locate the Faderpunk Drive</h3>
    <p className="mb-2">
      Your computer will now recognize the Faderpunk as a storage device (like a
      USB flash drive) named <strong>RP2350</strong>.
    </p>

    <p className="mt-4 font-bold">On Windows:</p>
    <ul className="mb-4 list-inside list-disc">
      <li>
        Open <strong>File Explorer</strong> (Windows key + E)
      </li>
      <li>Look in the left sidebar under "This PC" or "Devices and drives"</li>
      <li>
        You should see a drive labeled <strong>RP2350</strong>
      </li>
    </ul>

    <p className="mt-4 font-bold">On Mac:</p>
    <ul className="mb-4 list-inside list-disc">
      <li>
        A drive icon labeled <strong>RP2350</strong> should appear on your
        Desktop
      </li>
      <li>
        Alternatively, open <strong>Finder</strong> and look in the left sidebar
        under "Locations"
      </li>
    </ul>

    <p className="mt-4 font-bold">On Linux:</p>
    <ul className="mb-4 list-inside list-disc">
      <li>
        The drive should automatically mount and appear in your file manager
      </li>
      <li>
        Look for <strong>RP2350</strong> under mounted devices/volumes
      </li>
      <li>
        If it doesn't auto-mount, you may need to manually mount it from your
        file manager
      </li>
    </ul>

    <h3 className="mt-6 font-bold">Step 3: Install the Firmware</h3>
    <ol className="mb-4 list-inside list-decimal space-y-2">
      <li>
        <strong>Locate your firmware file</strong> (the <code>.uf2</code> file
        you downloaded)
      </li>
      <li>
        <strong>Drag and drop</strong> (or copy and paste) this{" "}
        <code>.uf2</code> file into the <strong>RP2350</strong> drive
        <ul className="mt-2 ml-6 list-inside list-disc space-y-1">
          <li>
            On <strong>Windows</strong>: Drag the file from your Downloads
            folder (or wherever you saved it) to the RP2350 drive in File
            Explorer
          </li>
          <li>
            On <strong>Mac</strong>: Drag the file from your Downloads folder
            (or Finder) to the RP2350 drive icon
          </li>
          <li>
            On <strong>Linux</strong>: Use your file manager to copy the{" "}
            <code>.uf2</code> file to the RP2350 drive
          </li>
        </ul>
      </li>
    </ol>

    <h3 className="mt-6 font-bold">Step 4: Automatic Reboot</h3>
    <ul className="mb-4 list-inside list-disc">
      <li>
        Once the file finishes copying, the{" "}
        <strong>RP2350 drive will disappear</strong> from your computer
      </li>
      <li>
        The Faderpunk will <strong>automatically reboot</strong> with the new
        firmware installed
      </li>
      <li>
        This happens within a few seconds — you don't need to do anything!
      </li>
    </ul>

    <h3 className="mt-6 font-bold">Step 5: Verify the Update</h3>
    <ol className="mb-4 list-inside list-decimal">
      <li>Connect your Faderpunk normally (without holding any buttons)</li>
      <li>
        Open the{" "}
        <a
          className="font-semibold underline"
          href="/configurator"
          target="_blank"
        >
          Faderpunk Configurator
        </a>{" "}
        in your browser, press the "Connect" button
      </li>
      <li>
        Check the firmware version number in the configurator to confirm the
        update was successful
      </li>
    </ol>

    <h3 className="mt-6 font-bold">Troubleshooting</h3>
    <p className="mt-4 font-bold">The RP2350 drive doesn't appear:</p>
    <ul className="mb-4 list-inside list-disc">
      <li>
        Make sure you're holding the Shift button <em>before</em> and{" "}
        <em>while</em> plugging in the USB cable
      </li>
      <li>Try a different USB cable or USB port</li>
      <li>
        On Linux, you may need to check if the drive needs to be manually
        mounted
      </li>
    </ul>

    <p className="mt-4 font-bold">The firmware doesn't seem to install:</p>
    <ul className="mb-4 list-inside list-disc">
      <li>
        Make sure you're copying the correct <code>.uf2</code> file (not a
        different file type)
      </li>
      <li>
        Ensure the file finishes copying completely before the device reboots
      </li>
      <li>Try the process again from Step 1</li>
    </ul>
  </>
);
