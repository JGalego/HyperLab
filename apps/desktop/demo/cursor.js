/**
 * A pointer you can see.
 *
 * Playwright moves a real mouse but records no cursor, so a film of it looks
 * like a haunting: fields fill themselves in and buttons press with nothing
 * touching them. This draws the pointer the recording is missing.
 *
 * Demo only. Nothing in HyperLab loads it.
 */

(() => {
  const install = () => {
    const cursor = document.createElement('div');
    cursor.setAttribute('data-demo-cursor', '');
    cursor.style.cssText = [
      'position:fixed',
      'left:0',
      'top:0',
      'width:14px',
      'height:14px',
      'margin:-7px 0 0 -7px',
      'border:2px solid #000',
      'border-radius:50%',
      'background:rgba(255,255,255,0.55)',
      'pointer-events:none',
      'z-index:2147483647',
      'transition:transform 40ms linear',
    ].join(';');
    document.body.append(cursor);

    addEventListener(
      'mousemove',
      (event) => {
        cursor.style.transform = `translate(${event.clientX}px, ${event.clientY}px)`;
      },
      true,
    );

    // A press is the one moment the pointer has to be unmistakable.
    addEventListener('mousedown', () => (cursor.style.background = '#000'), true);
    addEventListener(
      'mouseup',
      () => (cursor.style.background = 'rgba(255,255,255,0.55)'),
      true,
    );
  };

  if (document.body) install();
  else addEventListener('DOMContentLoaded', install);
})();
