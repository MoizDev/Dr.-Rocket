"use client";
import DemoRoot from "../DemoRoot";

export const LABEL = "The Visualizer replaying a binary search tree program at 2x: frames for each insert and tree-sum call appear, the tree is drawn node by node, and each step is explained in plain words.";

export default function VisualizerDemo({ className }: { className: string }) {
  return (
    <DemoRoot id="visualizer" className={className} w={1920} h={1362} label={LABEL}>
      {null}
    </DemoRoot>
  );
}
