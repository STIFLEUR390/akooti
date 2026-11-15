import { browser } from "wxt/browser";
import { defineBackground } from "wxt/utils/define-background";

/**
 * Service worker. Owns the Native Messaging connection to
 * `akooti native-host`; the full bridge lands in task 6.2.
 */
export default defineBackground(() => {
  browser.runtime.onInstalled.addListener(({ reason }) => {
    console.info(`[akooti] extension installed (${reason})`);
  });
});
