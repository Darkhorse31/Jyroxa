import assert from "node:assert/strict";
import {test} from "node:test";
import {videoProgressText} from "../src/video-progress.ts";
import {remainingTime} from "../src/sources.ts";

const base = {phase:"downloading",downloaded:25,total:100,total_is_estimate:false,speed_bps:5,eta_secs:15};
const text = p => videoProgressText(p, n=>`${n} B`, remainingTime);
test("video progress shows transferred, remaining, percentage and ETA", () => {
  assert.deepEqual(text(base), {summary:"25 B of 100 B, 75 B remaining, 15 seconds left",percent:"25.0%",size:"25 B of 100 B"});
  const estimate=text({...base,total_is_estimate:true});
  assert.equal(estimate.percent,"~25.0%");
  assert.match(estimate.summary,/of ~100 B, ~75 B remaining/);
});
test("unknown sizes and processing never pretend the task is complete", () => {
  const unknown=text({...base,total:null,eta_secs:null});
  assert.equal(unknown.percent,"—");
  assert.match(unknown.summary,/25 B downloaded, calculating total size, Calculating time left/);
  assert.equal(text({...base,phase:"fetching"}).summary,"Fetching video information…");
  const merging=text({...base,phase:"merging",downloaded:100});
  assert.equal(merging.percent,"100.0%");
  assert.match(merging.summary,/^Merging audio and video/);
  assert.doesNotMatch(merging.summary,/Completed|seconds left/);
  assert.match(text({...base,phase:"processing"}).summary,/^Processing video/);
});
