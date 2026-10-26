import { useEffect, useState } from "react";

// Roughly one screen of scrolling before the button is worth showing.
const SHOW_AFTER_PX = 600;

export const BackToTop = () => {
  const [visible, setVisible] = useState(false);

  useEffect(() => {
    const onScroll = () => setVisible(window.scrollY > SHOW_AFTER_PX);
    onScroll();
    window.addEventListener("scroll", onScroll, { passive: true });
    return () => window.removeEventListener("scroll", onScroll);
  }, []);

  if (!visible) return null;

  return (
    <button
      aria-label="Back to top"
      title="Back to top"
      className="bg-yellow-fp fixed right-6 bottom-6 z-20 flex h-12 w-12 cursor-pointer items-center justify-center rounded-sm text-black shadow-lg hover:brightness-110"
      onClick={() => window.scrollTo({ top: 0, behavior: "smooth" })}
    >
      <svg
        width="20"
        height="20"
        viewBox="0 0 20 20"
        fill="none"
        stroke="currentColor"
        strokeWidth="2.5"
        strokeLinecap="round"
        strokeLinejoin="round"
        aria-hidden="true"
      >
        <path d="M10 16V4M4 9l6-6 6 6" />
      </svg>
    </button>
  );
};
