import {
  ConfigMsgIn,
  ConfigMsgOut,
  deserialize,
  serialize,
} from "@atov/fp-config";

let receiveBuffer = new Uint8Array(0);

export function clearReceiveBuffer() {
  receiveBuffer = new Uint8Array(0);
}

const FRAME_DELIMITER = 0;
const FADERPUNK_VENDOR_ID = 0xf569;
const FADERPUNK_PRODUCT_ID = 0x1;
const USB_TRANSFER_SIZE = 512;

// TODO: use variable function style everywhere

export function cobsEncode(data: Uint8Array): Uint8Array {
  const maxSize = data.length + Math.ceil(data.length / 254) + 1;
  const encoded = new Uint8Array(maxSize);

  let codeIndex = 0;
  let writeIndex = 1;
  let code = 1;

  for (let i = 0; i < data.length; i++) {
    if (data[i] === 0) {
      encoded[codeIndex] = code;
      code = 1;
      codeIndex = writeIndex++;
    } else {
      encoded[writeIndex++] = data[i];
      code++;

      if (code === 255) {
        encoded[codeIndex] = code;
        code = 1;
        codeIndex = writeIndex++;
      }
    }
  }

  encoded[codeIndex] = code;

  return encoded.slice(0, writeIndex);
}

export function cobsDecode(data: Uint8Array): Uint8Array {
  if (data.length === 0) {
    return new Uint8Array(0);
  }

  const decoded = new Uint8Array(data.length);
  let writeIndex = 0;
  let readIndex = 0;

  while (readIndex < data.length) {
    const code = data[readIndex++];

    if (code === 0) {
      throw new Error("Invalid COBS-encoded data: zero code byte found");
    }

    for (let i = 1; i < code; i++) {
      if (readIndex >= data.length) {
        break;
      }
      decoded[writeIndex++] = data[readIndex++];
    }

    if (readIndex < data.length && code < 255) {
      decoded[writeIndex++] = 0;
    }
  }

  return decoded.slice(0, writeIndex);
}

function createMessageBuffer(msg: ConfigMsgIn): Uint8Array {
  const serialized = serialize("ConfigMsgIn", msg);
  const buf = new Uint8Array(serialized.length + 2);

  buf[0] = (serialized.length >> 8) & 0xff;
  buf[1] = serialized.length & 0xff;
  buf.set(serialized, 2);

  const cobsResult = cobsEncode(buf);
  const cobsEncoded = new Uint8Array(cobsResult.length + 1);

  cobsEncoded.set(cobsResult, 0);
  cobsEncoded[cobsEncoded.length - 1] = FRAME_DELIMITER;

  return cobsEncoded;
}

const getInterface = async (usbDevice: USBDevice) => {
  const iface = usbDevice.configuration?.interfaces.find((i) =>
    i.alternates.some((a) => a.interfaceClass === 0xff),
  );

  if (!iface) throw new Error("No webusb interface found");

  return iface;
};

export async function connectToFaderPunk(): Promise<USBDevice> {
  const usbDevice = await navigator.usb.requestDevice({
    filters: [
      {
        classCode: 0xff,
        vendorId: FADERPUNK_VENDOR_ID,
        productId: FADERPUNK_PRODUCT_ID,
      },
    ],
  });

  await usbDevice.open();
  if (!usbDevice.configuration) {
    await usbDevice.selectConfiguration(1);
  }

  const iface = await getInterface(usbDevice);

  await usbDevice.claimInterface(iface.interfaceNumber);

  clearReceiveBuffer();

  return usbDevice;
}

export async function sendMessage(
  usbDevice: USBDevice,
  msg: ConfigMsgIn,
): Promise<void> {
  const messageBuffer = createMessageBuffer(msg);
  const iface = await getInterface(usbDevice);

  await usbDevice.transferOut(iface.interfaceNumber, messageBuffer);
}

export async function receiveMessage(
  usbDevice: USBDevice,
): Promise<ConfigMsgOut> {
  while (true) {
    const delimiterPos = receiveBuffer.indexOf(FRAME_DELIMITER);
    if (delimiterPos !== -1) {
      const packet = receiveBuffer.slice(0, delimiterPos);
      receiveBuffer = receiveBuffer.slice(delimiterPos + 1);

      // An empty packet might be received, just continue
      if (packet.length === 0) {
        continue;
      }

      const cobsDecoded = cobsDecode(packet);
      // This can happen if we get a corrupted message
      if (cobsDecoded.length < 2) {
        console.error("Received corrupted message, skipping");
        continue;
      }
      const result = deserialize("ConfigMsgOut", cobsDecoded.slice(2));
      return result.value;
    }

    const iface = await getInterface(usbDevice);
    const data = await usbDevice.transferIn(
      iface.interfaceNumber,
      USB_TRANSFER_SIZE,
    );

    if (!data?.data?.buffer || data.data.byteLength === 0) {
      // This can happen on timeout, just try again
      continue;
    }

    const newData = new Uint8Array(
      data.data.buffer,
      data.data.byteOffset,
      data.data.byteLength,
    );
    const newBuffer = new Uint8Array(receiveBuffer.length + newData.length);
    newBuffer.set(receiveBuffer);
    newBuffer.set(newData, receiveBuffer.length);
    receiveBuffer = newBuffer;
  }
}

export async function sendAndReceive(
  usbDevice: USBDevice,
  msg: ConfigMsgIn,
): Promise<ConfigMsgOut> {
  await sendMessage(usbDevice, msg);

  return receiveMessage(usbDevice);
}

export async function receiveBatchMessages(
  usbDevice: USBDevice,
  count: bigint,
): Promise<ConfigMsgOut[]> {
  const messagePromises: Promise<ConfigMsgOut>[] = [];

  for (let i = 0; i < count; i++) {
    messagePromises.push(receiveMessage(usbDevice));
  }

  const results = await Promise.all(messagePromises);
  const endMessage = await receiveMessage(usbDevice);

  if (endMessage.tag !== "BatchMsgEnd") {
    throw new Error("Expected BatchMsgEnd but received: " + endMessage.tag);
  }

  return results;
}

export function getDeviceName(usbDevice: USBDevice): string {
  return `${usbDevice.manufacturerName} ${usbDevice.productName} v${usbDevice.deviceVersionMajor}.${usbDevice.deviceVersionMinor}.${usbDevice.deviceVersionSubminor}`;
}
