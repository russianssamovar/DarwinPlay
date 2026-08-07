#include <windows.h>
#include <algorithm>
#include <d3d11.h>
#include <d3dcompiler.h>
#include <dxgi.h>
#include <cstdio>
#include <cstdlib>
#include <cstring>

struct Vertex {
    float position[3];
    float color[4];
};

template <typename T>
void release(T*& value) {
    if (value) {
        value->Release();
        value = nullptr;
    }
}

LRESULT CALLBACK window_proc(HWND window, UINT message, WPARAM wparam, LPARAM lparam) {
    switch (message) {
    case WM_KEYDOWN:
        if (wparam == VK_ESCAPE) {
            DestroyWindow(window);
            return 0;
        }
        break;
    case WM_CLOSE:
        DestroyWindow(window);
        return 0;
    case WM_DESTROY:
        PostQuitMessage(0);
        return 0;
    default:
        break;
    }
    return DefWindowProcW(window, message, wparam, lparam);
}

bool compile_shader(const char* source, const char* entry, const char* target, ID3DBlob** blob) {
    ID3DBlob* errors = nullptr;
    const HRESULT result = D3DCompile(
        source,
        std::strlen(source),
        nullptr,
        nullptr,
        nullptr,
        entry,
        target,
        D3DCOMPILE_ENABLE_STRICTNESS,
        0,
        blob,
        &errors
    );
    if (FAILED(result)) {
        if (errors) {
            std::fprintf(stderr, "%.*s\n", static_cast<int>(errors->GetBufferSize()), static_cast<const char*>(errors->GetBufferPointer()));
        }
        release(errors);
        return false;
    }
    release(errors);
    return true;
}

int frame_limit(int argc, char** argv) {
    for (int index = 1; index + 1 < argc; ++index) {
        if (std::strcmp(argv[index], "--frames") == 0) {
            return std::max(0, std::atoi(argv[index + 1]));
        }
    }
    return 300;
}

int main(int argc, char** argv) {
    const HINSTANCE instance = GetModuleHandleW(nullptr);
    const wchar_t* class_name = L"DarwinPlayD3D11Fixture";

    WNDCLASSW window_class{};
    window_class.lpfnWndProc = window_proc;
    window_class.hInstance = instance;
    window_class.hCursor = LoadCursorW(nullptr, IDC_ARROW);
    window_class.lpszClassName = class_name;
    if (!RegisterClassW(&window_class)) {
        std::fprintf(stderr, "RegisterClassW failed: %lu\n", GetLastError());
        return 1;
    }

    RECT rect{0, 0, 960, 540};
    AdjustWindowRect(&rect, WS_OVERLAPPEDWINDOW, FALSE);
    HWND window = CreateWindowExW(
        0,
        class_name,
        L"DarwinPlay D3D11 Fixture",
        WS_OVERLAPPEDWINDOW,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        rect.right - rect.left,
        rect.bottom - rect.top,
        nullptr,
        nullptr,
        instance,
        nullptr
    );
    if (!window) {
        std::fprintf(stderr, "CreateWindowExW failed: %lu\n", GetLastError());
        return 2;
    }

    DXGI_SWAP_CHAIN_DESC swap_desc{};
    swap_desc.BufferDesc.Width = 960;
    swap_desc.BufferDesc.Height = 540;
    swap_desc.BufferDesc.Format = DXGI_FORMAT_R8G8B8A8_UNORM;
    swap_desc.SampleDesc.Count = 1;
    swap_desc.BufferUsage = DXGI_USAGE_RENDER_TARGET_OUTPUT;
    swap_desc.BufferCount = 2;
    swap_desc.OutputWindow = window;
    swap_desc.Windowed = TRUE;
    swap_desc.SwapEffect = DXGI_SWAP_EFFECT_DISCARD;

    D3D_FEATURE_LEVEL requested_levels[] = {
        D3D_FEATURE_LEVEL_11_0,
        D3D_FEATURE_LEVEL_10_1,
        D3D_FEATURE_LEVEL_10_0,
    };
    D3D_FEATURE_LEVEL feature_level{};
    IDXGISwapChain* swap_chain = nullptr;
    ID3D11Device* device = nullptr;
    ID3D11DeviceContext* context = nullptr;

    const HRESULT device_result = D3D11CreateDeviceAndSwapChain(
        nullptr,
        D3D_DRIVER_TYPE_HARDWARE,
        nullptr,
        0,
        requested_levels,
        static_cast<UINT>(sizeof(requested_levels) / sizeof(requested_levels[0])),
        D3D11_SDK_VERSION,
        &swap_desc,
        &swap_chain,
        &device,
        &feature_level,
        &context
    );
    if (FAILED(device_result)) {
        std::fprintf(stderr, "D3D11CreateDeviceAndSwapChain failed: 0x%08lx\n", static_cast<unsigned long>(device_result));
        DestroyWindow(window);
        return 3;
    }

    ID3D11Texture2D* back_buffer = nullptr;
    ID3D11RenderTargetView* render_target = nullptr;
    HRESULT result = swap_chain->GetBuffer(0, __uuidof(ID3D11Texture2D), reinterpret_cast<void**>(&back_buffer));
    if (SUCCEEDED(result)) {
        result = device->CreateRenderTargetView(back_buffer, nullptr, &render_target);
    }
    release(back_buffer);
    if (FAILED(result)) {
        std::fprintf(stderr, "Render target creation failed: 0x%08lx\n", static_cast<unsigned long>(result));
        release(context);
        release(device);
        release(swap_chain);
        DestroyWindow(window);
        return 4;
    }

    const char* shader_source = R"(
struct VSInput { float3 position : POSITION; float4 color : COLOR; };
struct VSOutput { float4 position : SV_POSITION; float4 color : COLOR; };
VSOutput VSMain(VSInput input) {
    VSOutput output;
    output.position = float4(input.position, 1.0);
    output.color = input.color;
    return output;
}
float4 PSMain(VSOutput input) : SV_TARGET { return input.color; }
)";

    ID3DBlob* vertex_blob = nullptr;
    ID3DBlob* pixel_blob = nullptr;
    if (!compile_shader(shader_source, "VSMain", "vs_5_0", &vertex_blob) ||
        !compile_shader(shader_source, "PSMain", "ps_5_0", &pixel_blob)) {
        release(vertex_blob);
        release(pixel_blob);
        release(render_target);
        release(context);
        release(device);
        release(swap_chain);
        DestroyWindow(window);
        return 5;
    }

    ID3D11VertexShader* vertex_shader = nullptr;
    ID3D11PixelShader* pixel_shader = nullptr;
    ID3D11InputLayout* input_layout = nullptr;
    result = device->CreateVertexShader(vertex_blob->GetBufferPointer(), vertex_blob->GetBufferSize(), nullptr, &vertex_shader);
    if (SUCCEEDED(result)) {
        result = device->CreatePixelShader(pixel_blob->GetBufferPointer(), pixel_blob->GetBufferSize(), nullptr, &pixel_shader);
    }

    D3D11_INPUT_ELEMENT_DESC input_elements[] = {
        {"POSITION", 0, DXGI_FORMAT_R32G32B32_FLOAT, 0, 0, D3D11_INPUT_PER_VERTEX_DATA, 0},
        {"COLOR", 0, DXGI_FORMAT_R32G32B32A32_FLOAT, 0, 12, D3D11_INPUT_PER_VERTEX_DATA, 0},
    };
    if (SUCCEEDED(result)) {
        result = device->CreateInputLayout(
            input_elements,
            static_cast<UINT>(sizeof(input_elements) / sizeof(input_elements[0])),
            vertex_blob->GetBufferPointer(),
            vertex_blob->GetBufferSize(),
            &input_layout
        );
    }
    release(vertex_blob);
    release(pixel_blob);
    if (FAILED(result)) {
        std::fprintf(stderr, "Shader pipeline creation failed: 0x%08lx\n", static_cast<unsigned long>(result));
        release(input_layout);
        release(pixel_shader);
        release(vertex_shader);
        release(render_target);
        release(context);
        release(device);
        release(swap_chain);
        DestroyWindow(window);
        return 6;
    }

    const Vertex vertices[] = {
        {{0.0f, 0.72f, 0.0f}, {1.0f, 0.15f, 0.12f, 1.0f}},
        {{0.72f, -0.62f, 0.0f}, {0.12f, 1.0f, 0.28f, 1.0f}},
        {{-0.72f, -0.62f, 0.0f}, {0.16f, 0.38f, 1.0f, 1.0f}},
    };
    D3D11_BUFFER_DESC buffer_desc{};
    buffer_desc.ByteWidth = sizeof(vertices);
    buffer_desc.Usage = D3D11_USAGE_IMMUTABLE;
    buffer_desc.BindFlags = D3D11_BIND_VERTEX_BUFFER;
    D3D11_SUBRESOURCE_DATA initial_data{};
    initial_data.pSysMem = vertices;
    ID3D11Buffer* vertex_buffer = nullptr;
    result = device->CreateBuffer(&buffer_desc, &initial_data, &vertex_buffer);
    if (FAILED(result)) {
        std::fprintf(stderr, "CreateBuffer failed: 0x%08lx\n", static_cast<unsigned long>(result));
        release(input_layout);
        release(pixel_shader);
        release(vertex_shader);
        release(render_target);
        release(context);
        release(device);
        release(swap_chain);
        DestroyWindow(window);
        return 7;
    }

    D3D11_VIEWPORT viewport{};
    viewport.Width = 960.0f;
    viewport.Height = 540.0f;
    viewport.MinDepth = 0.0f;
    viewport.MaxDepth = 1.0f;

    ShowWindow(window, SW_SHOWDEFAULT);
    UpdateWindow(window);
    std::printf("ready feature_level=0x%04x\n", static_cast<unsigned int>(feature_level));
    std::fflush(stdout);

    const UINT stride = sizeof(Vertex);
    const UINT offset = 0;
    const float clear_color[] = {0.025f, 0.03f, 0.045f, 1.0f};
    const int max_frames = frame_limit(argc, argv);
    int frames = 0;
    bool running = true;

    while (running) {
        MSG message{};
        while (PeekMessageW(&message, nullptr, 0, 0, PM_REMOVE)) {
            if (message.message == WM_QUIT) {
                running = false;
                break;
            }
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        if (!running) {
            break;
        }

        context->OMSetRenderTargets(1, &render_target, nullptr);
        context->RSSetViewports(1, &viewport);
        context->ClearRenderTargetView(render_target, clear_color);
        context->IASetInputLayout(input_layout);
        context->IASetPrimitiveTopology(D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
        context->IASetVertexBuffers(0, 1, &vertex_buffer, &stride, &offset);
        context->VSSetShader(vertex_shader, nullptr, 0);
        context->PSSetShader(pixel_shader, nullptr, 0);
        context->Draw(3, 0);

        result = swap_chain->Present(1, 0);
        if (FAILED(result)) {
            std::fprintf(stderr, "Present failed: 0x%08lx\n", static_cast<unsigned long>(result));
            running = false;
            break;
        }

        ++frames;
        if (max_frames > 0 && frames >= max_frames) {
            running = false;
        }
    }

    std::printf("completed frames=%d\n", frames);
    std::fflush(stdout);
    release(vertex_buffer);
    release(input_layout);
    release(pixel_shader);
    release(vertex_shader);
    release(render_target);
    release(context);
    release(device);
    release(swap_chain);
    if (IsWindow(window)) {
        DestroyWindow(window);
    }
    UnregisterClassW(class_name, instance);
    return FAILED(result) ? 8 : 0;
}
