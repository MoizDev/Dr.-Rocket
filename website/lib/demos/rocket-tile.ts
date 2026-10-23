// The pasted rocket image in poster.rkt, drawn as a small inline SVG (no image file).
export function rocketTile(): string {
  return (
    '<svg viewBox="0 0 24 24" width="100%" height="100%" aria-hidden="true">' +
    '<rect width="24" height="24" rx="4" fill="#1d2a44"/>' +
    '<path d="M12 3c3 2 4.5 5.5 4.5 9.5L15 17H9l-1.5-4.5C7.5 8.5 9 5 12 3z" fill="#e8ecf4"/>' +
    '<circle cx="12" cy="10" r="2" fill="#5aa9ff"/>' +
    '<path d="M9 17l-2.5 3 1-4.5zM15 17l2.5 3-1-4.5z" fill="#ff7a59"/>' +
    '<path d="M10.5 17.5h3L12 21z" fill="#ffc857"/>' +
    "</svg>"
  );
}
