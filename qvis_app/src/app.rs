use leptos::prelude::*;
use leptos_meta::*;

#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();

    view! { <Home /> }
}

#[component]
fn Home() -> impl IntoView {
    let (value, set_value) = signal(0);

    view! {
      <main>
        <div class="flex flex-col min-h-screen font-mono text-white from-blue-800 to-blue-500 bg-linear-to-tl">
          <div class="flex flex-row-reverse flex-wrap m-auto">
            <button
              on:click=move |_| set_value.update(|value| *value += 1)
              class="py-2 px-3 m-1 text-white bg-blue-700 rounded border-l-2 border-b-4 border-blue-800 shadow-lg"
            >
              "+"
            </button>
            <button class="py-2 px-3 m-1 text-white bg-blue-800 rounded border-l-2 border-b-4 border-blue-900 shadow-lg">
              {value}
            </button>
            <button
              on:click=move |_| set_value.update(|value| *value -= 1)
              class="py-2 px-3 m-1 text-white bg-blue-700 rounded border-l-2 border-b-4 border-blue-800 shadow-lg"
              class:invisible=move || { value.get() < 1 }
            >
              "-"
            </button>
          </div>
        </div>
      </main>
    }
}
