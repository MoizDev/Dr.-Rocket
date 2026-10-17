"use client";

import { useEffect } from "react";

/** Marks the page as scrolled (`<html data-scrolled>`) once it moves, so the header compacts. */
export default function NavScroll() {
  useEffect(() => {
    const root = document.documentElement;
    const update = () => root.toggleAttribute("data-scrolled", window.scrollY > 24);
    update();
    window.addEventListener("scroll", update, { passive: true });
    return () => window.removeEventListener("scroll", update);
  }, []);
  return null;
}
