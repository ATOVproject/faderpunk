import { H2 } from "./Shared";

// Deliberately independent of faderpunk-community-apps' own build/render
// stack (see Package H's plan) — the iframe target is whatever static
// site that repo publishes to GitHub Pages, kept in sync with its own
// release schedule, not this repo's. This section renders regardless of
// what (if anything) is installed on the connected device: it's a
// discovery/promo page for the whole catalogue, not per-device state.
//
// #/manual matches the route the old (pre-Package-H) manual link used —
// a reasonable guess pending H2's actual implementation there, not a
// confirmed contract. Update once that repo's catalogue site exists for
// real and its route (if any) is known.
const CATALOGUE_URL =
  "https://atovproject.github.io/faderpunk-community-apps/#/manual";

export const CommunityCatalogue = () => (
  <>
    <H2 id="community-catalogue">Community App Catalogue</H2>
    <p className="mb-6">
      Every app in the community catalog, installed or not — browse manuals and
      download <code>.fpapp</code> files directly.
    </p>
    <iframe
      src={CATALOGUE_URL}
      title="Community App Catalogue"
      className="h-[80vh] w-full rounded-sm border border-white/10"
    />
  </>
);
