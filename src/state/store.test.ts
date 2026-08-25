import { beforeEach, describe, expect, it } from "vitest";
import { useApp } from "./store";
import type { JobInfo } from "../lib/types";

const job = (id: string, state: JobInfo["state"] = "running"): JobInfo => ({
  id,
  kind: "test",
  label: `job ${id}`,
  state,
  progress: null,
  detail: null,
  error: null,
  createdAt: 1,
  finishedAt: null,
});

describe("terminal tabs", () => {
  beforeEach(() => {
    useApp.setState({ termTabs: [], activeTermId: null, view: "hosts" });
  });

  it("adding a tab activates it and switches to the terminal view", () => {
    useApp.getState().addTermTab({ termId: "t1", sessionId: "s1", title: "web" });
    const s = useApp.getState();
    expect(s.termTabs).toHaveLength(1);
    expect(s.activeTermId).toBe("t1");
    expect(s.view).toBe("terminal");
  });

  it("closing the active tab falls back to the last remaining tab", () => {
    const { addTermTab, closeTermTab } = useApp.getState();
    addTermTab({ termId: "t1", sessionId: "s1", title: "a" });
    addTermTab({ termId: "t2", sessionId: "s1", title: "b" });
    addTermTab({ termId: "t3", sessionId: "s1", title: "c" });
    closeTermTab("t3");
    expect(useApp.getState().activeTermId).toBe("t2");
    // closing an inactive tab keeps the active one
    closeTermTab("t1");
    expect(useApp.getState().activeTermId).toBe("t2");
    closeTermTab("t2");
    expect(useApp.getState().activeTermId).toBeNull();
  });
});

describe("job updates", () => {
  beforeEach(() => {
    useApp.setState({ jobs: [] });
  });

  it("inserts new jobs at the front and updates existing in place", () => {
    const { upsertJob } = useApp.getState();
    upsertJob(job("a"));
    upsertJob(job("b"));
    expect(useApp.getState().jobs.map((j) => j.id)).toEqual(["b", "a"]);

    upsertJob({ ...job("a"), state: "done" });
    const jobs = useApp.getState().jobs;
    expect(jobs).toHaveLength(2);
    expect(jobs.find((j) => j.id === "a")?.state).toBe("done");
    // order preserved on update
    expect(jobs.map((j) => j.id)).toEqual(["b", "a"]);
  });
});
