# showroom-recorder

Record showroom live and uploads the segments to s3.

## Script to extract all members from campaign page

```js
Array.from(document.querySelectorAll(".room-card"))
  .map((r) => {
    const roomSlug = r.querySelector("a").href.split("/")[3].split("?")[0];
    const roomName = r.querySelector(".room-card__text--main").innerText;
    return `"${roomSlug}", # ${roomName}`;
  })
  .join("\n");
```
