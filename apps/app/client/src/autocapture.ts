import slugify from "slugify";
import type { ScryerClient } from "./index";

class Autocapture {
  private handledButtons = new Set();
  private handledLinks = new Set();
  private handledForms = new Set();
  private regexPatterns = [{ pattern: /email/i, fieldName: "email" }];
  private client: ScryerClient;
  private observer: MutationObserver;
  private scrollThresholds: boolean[] = [false, false, false, false];

  constructor(client: ScryerClient) {
    this.client = client;
    this.observer = new MutationObserver(() => this.addEventListeners());
  }

  findFirstMatch(inputs) {
    for (const entry of this.regexPatterns) {
      if (inputs.some((input) => entry.pattern.test(input))) {
        return entry.fieldName;
      }
    }
    return null;
  }

  findForm(elem) {
    while (elem) {
      if (elem.nodeName === "FORM") return elem;
      elem = elem.parentNode;
    }
    return null;
  }

  extractFormIdentifier(form) {
    return (
      form.getAttribute("name") ||
      form.getAttribute("id") ||
      form.getAttribute("data-form-name") ||
      this.extractFromButton(form) ||
      undefined
    );
  }

  extractFromButton(form) {
    const buttons: any[] = Array.from(
      form.querySelectorAll("button, input[type='submit']")
    );
    const submitButton =
      buttons.find((btn) => btn.getAttribute("type") === "submit") ||
      buttons.pop();
    return (
      submitButton?.innerText?.trim() ||
      submitButton?.getAttribute("aria-label")?.trim()
    );
  }

  processFormData(form) {
    const formData = new FormData(form);
    const data: any = {};

    formData.forEach((value, key) => {
      const field: any = document.querySelector(`[name='${key}']`);
      if (!field) return;

      const checks = [field.type, field.placeholder, field.getAttribute("id")];
      const fieldName =
        this.findFirstMatch(checks) ||
        slugify(key, { strict: true, lower: true });
      data[fieldName] = value;
    });

    data.formName = this.extractFormIdentifier(form);
    return data;
  }

  handleFormSubmit(event) {
    const form = event.target;
    if (this.handledForms.has(form)) return;

    this.handledForms.add(form);
    setTimeout(() => this.handledForms.delete(form), 500);

    if (!form.checkValidity()) return;
    const formData = this.processFormData(form);
    this.client.sendEvent({ event: "form_submission", formData }, true);
  }

  handleButtonClick(event) {
    const button = event.target.closest("button");
    if (button.disabled) return;
    if (this.handledButtons.has(button)) return;
    this.handledButtons.add(button);
    setTimeout(() => this.handledButtons.delete(button), 500);
    const label =
      button.getAttribute("aria-label")?.trim() ||
      button.innerText?.trim() ||
      button.getAttribute("name");

    this.client.sendEvent({
      event: "button_click",
      clickData: { label: label || undefined },
    });
  }

  handleLinkClick(event) {
    const link = event.target.closest("a");
    if (link.disabled) return;
    if (this.handledLinks.has(link)) return;
    this.handledLinks.add(link);
    setTimeout(() => this.handledLinks.delete(link), 500);
    const label =
      link.getAttribute("aria-label")?.trim() ||
      link.innerText?.trim() ||
      link.getAttribute("name");

    this.client.sendEvent({
      event: "link_click",
      clickData: { label: label || undefined, href: link.href },
    });
  }

  handleScroll() {
    const scrollHeight = document.documentElement.scrollHeight;
    const viewportHeight = window.innerHeight;

    if (scrollHeight <= viewportHeight) return; // No scrolling possible

    const scrollTop = window.scrollY || document.documentElement.scrollTop;
    const scrollPosition = scrollTop + viewportHeight;
    const scrollPercentage =
      (scrollTop / (scrollHeight - viewportHeight)) * 100;

    if (scrollPercentage >= 25 && !this.scrollThresholds[0]) {
      this.scrollThresholds[0] = true;
      this.client.sendEvent({ event: "scroll_25_percent" });
    }

    if (scrollPercentage >= 50 && !this.scrollThresholds[1]) {
      this.scrollThresholds[1] = true;
      this.client.sendEvent({ event: "scroll_50_percent" });
    }

    if (scrollPercentage >= 75 && !this.scrollThresholds[2]) {
      this.scrollThresholds[2] = true;
      this.client.sendEvent({ event: "scroll_75_percent" });
    }

    if (scrollPosition >= scrollHeight && !this.scrollThresholds[3]) {
      this.scrollThresholds[3] = true;
      this.client.sendEvent({ event: "scroll_bottom" });
    }
  }

  addEventListeners() {
    document.querySelectorAll("button").forEach((button) => {
      button.removeEventListener("click", this.handleButtonClick.bind(this));
      button.addEventListener("click", this.handleButtonClick.bind(this), {
        capture: true,
        passive: true,
      });
    });

    document.querySelectorAll("a").forEach((link) => {
      link.removeEventListener("click", this.handleLinkClick.bind(this));
      link.addEventListener("click", this.handleLinkClick.bind(this), {
        capture: true,
        passive: true,
      });
    });

    document.querySelectorAll("form").forEach((form) => {
      form.removeEventListener("submit", this.handleFormSubmit.bind(this));
      form.addEventListener("submit", this.handleFormSubmit.bind(this), {
        capture: true,
        passive: true,
      });
    });
  }

  startTracking() {
    this.observer.observe(document, { childList: true, subtree: true });
    this.addEventListeners();

    // window.addEventListener("scroll", this.handleScroll.bind(this), {
    //   passive: true,
    // });
  }
}

export default Autocapture;
