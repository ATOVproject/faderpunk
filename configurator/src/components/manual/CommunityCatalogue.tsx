import { useCallback, useEffect, useRef, useState } from "react";

import { H2 } from "./Shared";

// Deliberately independent of faderpunk-community-apps' own build/render
// stack (see Package H's plan) — the iframe target is whatever static
// site that repo publishes to GitHub Pages, kept in sync with its own
// release schedule, not this repo's. This section renders regardless of
// what (if anything) is installed on the connected device: it's a
// discovery/promo page for the whole catalogue, not per-device state.
const CATALOGUE_URL = "https://atovproject.github.io/faderpunk-community-apps/";
const CATALOGUE_ORIGIN = new URL(CATALOGUE_URL).origin;

// Only until the catalogue reports its real height. Deliberately tall:
// if the handshake below never completes (old cached copy of the
// catalogue, script blocked), this stays scrollable at a usable size
// rather than clipping the content away.
const FALLBACK_HEIGHT = 2400;

export const CommunityCatalogue = () => {
  const [height, setHeight] = useState<number | undefined>(undefined);
  const frame = useRef<HTMLIFrameElement>(null);

  // Ask the catalogue how tall it is. It answers on load too, but that
  // races this component mounting — whoever is late would otherwise never
  // hear the other, so the request is what actually makes it reliable.
  const requestHeight = useCallback(() => {
    frame.current?.contentWindow?.postMessage(
      { type: "fp-catalogue-request-height" },
      CATALOGUE_ORIGIN,
    );
  }, []);

  // Cross-origin, so the height can't be measured from here — the
  // catalogue reports its own (see tools/manual-site/src/entry.tsx in
  // faderpunk-community-apps) and the frame grows to fit, so the reader
  // scrolls one page instead of a box inside a page.
  useEffect(() => {
    const onMessage = (event: MessageEvent) => {
      if (event.origin !== CATALOGUE_ORIGIN) return;
      if (event.data?.type !== "fp-catalogue-height") return;
      const reported = Number(event.data.height);
      if (Number.isFinite(reported) && reported > 0) setHeight(reported);
    };
    window.addEventListener("message", onMessage);
    requestHeight();
    return () => window.removeEventListener("message", onMessage);
  }, [requestHeight]);

  return (
    <>
      <H2 id="community-catalogue">Community App Catalogue</H2>
      <p className="mb-6">
        Every app in the community catalog, installed or not — browse manuals
        and download <code>.fpapp</code> files directly.
      </p>
      <iframe
        ref={frame}
        src={CATALOGUE_URL}
        title="Community App Catalogue"
        onLoad={requestHeight}
        // Only stop scrolling once we know the frame is tall enough to
        // show everything; until then it must stay reachable.
        scrolling={height ? "no" : "yes"}
        style={{ height: height ?? FALLBACK_HEIGHT }}
        className="w-full border-0"
      />
    </>
  );
};
