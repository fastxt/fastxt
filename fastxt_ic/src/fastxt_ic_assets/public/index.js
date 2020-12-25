import fastxt_ic from 'ic:canisters/fastxt_ic';

fastxt_ic.greet(window.prompt("Enter your name:")).then(greeting => {
  window.alert(greeting);
});
