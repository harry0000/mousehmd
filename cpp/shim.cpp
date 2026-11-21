// OpenVR-facing half of the mousehmd driver.
//
// OpenVR's driver API is a set of C++ abstract classes, so the classes SteamVR
// talks to are implemented here. Everything stateful about input lives in Rust
// (src/input.rs); this file only asks it for the current head rotation.

#include "openvr_driver.h"

#include <atomic>
#include <chrono>
#include <cmath>
#include <cstdarg>
#include <cstdio>
#include <cstring>
#include <system_error>
#include <thread>

using namespace vr;

// ---- Rust side (src/lib.rs) -------------------------------------------------

extern "C" {
struct MouseHmdConfig {
    float mouseDegPerPx;
    float keyDegPerSec;
    bool grabStyle;
    bool anyWindow;
    float smoothingMs;
};
// Dimensions are signed on purpose: a negative setting has to reach the range
// check as a negative number, not as a huge unsigned one.
struct MouseHmdDisplayConfig {
    int32_t windowWidth;
    int32_t windowHeight;
    float horizontalFovDeg;
    float renderScale;
    float displayFrequency;
    float ipdMeters;
    float headHeightMeters;
};
// A window rectangle in physical desktop pixels.
struct MouseHmdWindowRect {
    int32_t x;
    int32_t y;
    int32_t width;
    int32_t height;
};
// Returns false if the input thread could not be started.
bool mousehmd_input_start(const MouseHmdConfig *cfg);
void mousehmd_input_stop();
void mousehmd_get_rotation(double *outWxyz);
// Whether the key standing in for the controller system button is held.
bool mousehmd_system_button();
// Clamps every field to a range that is safe to hand to SteamVR.
void mousehmd_sanitize_display(MouseHmdDisplayConfig *display);
// The headset window: its size from the windowWidth/windowHeight settings
// (0 = not set) or the primary monitor, and the position SteamVR gives it.
// Returns false if the screen could not be read; `out` then holds the
// fallback size.
bool mousehmd_headset_window(int32_t width, int32_t height, MouseHmdWindowRect *out);
}

// ---- helpers ----------------------------------------------------------------

static const char *const kSection = "driver_mousehmd";
static const char *const kSerial = "MOUSEHMD-0001";
static const char *const kModel = "Mouse HMD";

static void Log(const char *fmt, ...) {
    char buf[512];
    va_list args;
    va_start(args, fmt);
    vsnprintf(buf, sizeof(buf), fmt, args);
    va_end(args);
    if (VRDriverLog()) {
        VRDriverLog()->Log(buf);
    }
}

static float SettingFloat(const char *key, float fallback) {
    EVRSettingsError err = VRSettingsError_None;
    float v = VRSettings()->GetFloat(kSection, key, &err);
    return err == VRSettingsError_None ? v : fallback;
}

static int32_t SettingInt(const char *key, int32_t fallback) {
    EVRSettingsError err = VRSettingsError_None;
    int32_t v = VRSettings()->GetInt32(kSection, key, &err);
    return err == VRSettingsError_None ? v : fallback;
}

static bool SettingBool(const char *key, bool fallback) {
    EVRSettingsError err = VRSettingsError_None;
    bool v = VRSettings()->GetBool(kSection, key, &err);
    return err == VRSettingsError_None ? v : fallback;
}

// ---- the virtual headset ----------------------------------------------------

class MouseHmd : public ITrackedDeviceServerDriver, public IVRDisplayComponent {
public:
    void LoadSettings() {
        m_monoView = SettingBool("monoView", true);

        // Size and position are worked out in Rust (src/logic.rs): the size
        // from the settings or the primary monitor, the position where SteamVR
        // puts the window anyway. SteamVR keeps this size until it restarts.
        MouseHmdWindowRect window = {};
        if (!mousehmd_headset_window(SettingInt("windowWidth", 0), SettingInt("windowHeight", 0), &window)) {
            Log("mousehmd: could not read the screen size; the default window size falls back to 1280x720\n");
        }

        MouseHmdDisplayConfig display;
        display.windowWidth = window.width;
        display.windowHeight = window.height;
        display.horizontalFovDeg = SettingFloat("horizontalFovDeg", 100.0f);
        display.renderScale = SettingFloat("renderScale", 1.0f);
        display.displayFrequency = SettingFloat("displayFrequency", 60.0f);
        display.ipdMeters = SettingFloat("ipdMeters", 0.063f);
        display.headHeightMeters = SettingFloat("headHeightMeters", 0.0f);
        // Range checks live in Rust (src/logic.rs), where they are unit-tested.
        // Afterwards both dimensions are positive and small enough that the
        // conversions below, and the scaled render target size, cannot overflow.
        mousehmd_sanitize_display(&display);

        // The size is already within range, so the position computed for it
        // still holds after the checks above.
        m_windowX = window.x;
        m_windowY = window.y;
        m_windowWidth = (uint32_t)display.windowWidth;
        m_windowHeight = (uint32_t)display.windowHeight;
        m_horizontalFovDeg = display.horizontalFovDeg;
        m_renderScale = display.renderScale;
        m_displayFrequency = display.displayFrequency;
        m_ipdMeters = display.ipdMeters;
        m_headHeightMeters = display.headHeightMeters;
    }

    // ITrackedDeviceServerDriver

    EVRInitError Activate(uint32_t unObjectId) override {
        m_deviceIndex = unObjectId;
        PropertyContainerHandle_t props = VRProperties()->TrackedDeviceToPropertyContainer(unObjectId);

        VRProperties()->SetStringProperty(props, Prop_ModelNumber_String, kModel);
        VRProperties()->SetFloatProperty(props, Prop_UserIpdMeters_Float, m_ipdMeters);
        VRProperties()->SetFloatProperty(props, Prop_UserHeadToEyeDepthMeters_Float, 0.0f);
        VRProperties()->SetFloatProperty(props, Prop_DisplayFrequency_Float, m_displayFrequency);
        VRProperties()->SetFloatProperty(props, Prop_SecondsFromVsyncToPhotons_Float, 0.011f);
        // Any value other than 0 (invalid) and 1 (reserved for Oculus).
        VRProperties()->SetUint64Property(props, Prop_CurrentUniverseId_Uint64, 2);
        // Same combination Valve's sample virtual HMD uses for a desktop window.
        VRProperties()->SetBoolProperty(props, Prop_IsOnDesktop_Bool, false);
        VRProperties()->SetBoolProperty(props, Prop_DisplayDebugMode_Bool, true);

        // Report a proximity sensor that always says "worn". Without it SteamVR
        // decides the headset was taken off whenever the view stops moving for a
        // few seconds: it enters standby and tells the app the user is gone,
        // which makes players pause and pop up their menu.
        VRProperties()->SetBoolProperty(props, Prop_ContainsProximitySensor_Bool, true);
        if (VRDriverInput()->CreateBooleanComponent(props, "/proximity", &m_proximity) == VRInputError_None) {
            VRDriverInput()->UpdateBooleanComponent(m_proximity, true, 0.0);
        } else {
            m_proximity = k_ulInvalidInputComponentHandle;
            Log("mousehmd: could not create /proximity component\n");
        }

        // SteamVR reserves /input/system/click for summoning and dismissing the
        // dashboard. With no controller this is the only way to close a dashboard
        // that opened. Creating the component is not enough on a headset: SteamVR's
        // stock dashboard bindings for a generic HMD do not listen to it, so the
        // input profile ships a binding that maps it to ToggleDashboard.
        VRProperties()->SetStringProperty(props, Prop_ControllerType_String, "mousehmd_hmd");
        VRProperties()->SetStringProperty(props, Prop_InputProfilePath_String, "{mousehmd}/input/mousehmd_profile.json");
        if (VRDriverInput()->CreateBooleanComponent(props, "/input/system/click", &m_systemButton) != VRInputError_None) {
            m_systemButton = k_ulInvalidInputComponentHandle;
            Log("mousehmd: could not create /input/system/click component\n");
        }

        // std::thread throws if the OS cannot start the thread. Nothing may be
        // thrown back into vrserver, so report it as a failed activation instead,
        // as Init does when the Rust input thread cannot start.
        m_running = true;
        try {
            m_poseThread = std::thread([this] { PoseLoop(); });
        } catch (const std::system_error &e) {
            m_running = false;
            m_deviceIndex = k_unTrackedDeviceIndexInvalid;
            Log("mousehmd: could not start the pose thread: %s\n", e.what());
            return VRInitError_Driver_Failed;
        }
        Log("mousehmd: activated as device %u, window %dx%d at (%d,%d), mono=%d, hfov=%.1f\n",
            unObjectId, m_windowWidth, m_windowHeight, m_windowX, m_windowY,
            (int)m_monoView, m_horizontalFovDeg);
        return VRInitError_None;
    }

    void Deactivate() override {
        StopPoseThread();
        m_deviceIndex = k_unTrackedDeviceIndexInvalid;
    }

    void EnterStandby() override {}

    void *GetComponent(const char *pchComponentNameAndVersion) override {
        if (strcmp(pchComponentNameAndVersion, IVRDisplayComponent_Version) == 0) {
            return static_cast<IVRDisplayComponent *>(this);
        }
        return nullptr;
    }

    void DebugRequest(const char *, char *pchResponseBuffer, uint32_t unResponseBufferSize) override {
        if (unResponseBufferSize >= 1) pchResponseBuffer[0] = 0;
    }

    DriverPose_t GetPose() override { return MakePose(); }

    // IVRDisplayComponent

    void GetWindowBounds(int32_t *pnX, int32_t *pnY, uint32_t *pnWidth, uint32_t *pnHeight) override {
        *pnX = m_windowX;
        *pnY = m_windowY;
        *pnWidth = m_windowWidth;
        *pnHeight = m_windowHeight;
    }

    bool IsDisplayOnDesktop() override { return true; }
    bool IsDisplayRealDisplay() override { return false; }

    void GetRecommendedRenderTargetSize(uint32_t *pnWidth, uint32_t *pnHeight) override {
        *pnWidth = (uint32_t)(EyeViewportWidth() * m_renderScale);
        *pnHeight = (uint32_t)(m_windowHeight * m_renderScale);
    }

    void GetEyeOutputViewport(EVREye eEye, uint32_t *pnX, uint32_t *pnY, uint32_t *pnWidth,
                              uint32_t *pnHeight) override {
        *pnY = 0;
        *pnHeight = m_windowHeight;
        *pnWidth = EyeViewportWidth();
        // Mono: both eyes cover the whole window, so one eye's picture fills it.
        *pnX = (m_monoView || eEye == Eye_Left) ? 0 : m_windowWidth / 2;
    }

    void GetProjectionRaw(EVREye, float *pfLeft, float *pfRight, float *pfTop, float *pfBottom) override {
        const float kPi = 3.14159265358979f;
        float tanX = std::tan(m_horizontalFovDeg * 0.5f * kPi / 180.0f);
        float tanY = tanX * (float)m_windowHeight / (float)EyeViewportWidth();
        *pfLeft = -tanX;
        *pfRight = tanX;
        *pfTop = -tanY;
        *pfBottom = tanY;
    }

    // A flat window has no lens, so no distortion.
    DistortionCoordinates_t ComputeDistortion(EVREye, float fU, float fV) override {
        DistortionCoordinates_t c;
        c.rfRed[0] = c.rfGreen[0] = c.rfBlue[0] = fU;
        c.rfRed[1] = c.rfGreen[1] = c.rfBlue[1] = fV;
        return c;
    }

    bool ComputeInverseDistortion(HmdVector2_t *pResult, EVREye, uint32_t, float fU, float fV) override {
        pResult->v[0] = fU;
        pResult->v[1] = fV;
        return true;
    }

    void StopPoseThread() {
        if (m_running.exchange(false) && m_poseThread.joinable()) {
            m_poseThread.join();
        }
    }

private:
    uint32_t EyeViewportWidth() const { return m_monoView ? m_windowWidth : m_windowWidth / 2; }

    DriverPose_t MakePose() const {
        DriverPose_t pose = {};
        pose.poseIsValid = true;
        pose.deviceIsConnected = true;
        pose.result = TrackingResult_Running_OK;
        pose.qWorldFromDriverRotation.w = 1.0;
        pose.qDriverFromHeadRotation.w = 1.0;
        pose.vecPosition[1] = m_headHeightMeters;

        double q[4] = {1.0, 0.0, 0.0, 0.0};
        mousehmd_get_rotation(q);
        pose.qRotation.w = q[0];
        pose.qRotation.x = q[1];
        pose.qRotation.y = q[2];
        pose.qRotation.z = q[3];
        return pose;
    }

    void PoseLoop() {
        uint32_t tick = 0;
        bool systemHeld = false;
        while (m_running) {
            uint32_t index = m_deviceIndex;
            if (index != k_unTrackedDeviceIndexInvalid) {
                VRServerDriverHost()->TrackedDevicePoseUpdated(index, MakePose(), sizeof(DriverPose_t));
                // Forward the system button on every change of state.
                bool held = mousehmd_system_button();
                if (held != systemHeld && m_systemButton != k_ulInvalidInputComponentHandle) {
                    systemHeld = held;
                    EVRInputError err = VRDriverInput()->UpdateBooleanComponent(m_systemButton, held, 0.0);
                    Log("mousehmd: system button %s (error %d)\n", held ? "down" : "up", (int)err);
                }
                // Refresh "worn" about once a second so it never goes stale.
                if (++tick % 500 == 0 && m_proximity != k_ulInvalidInputComponentHandle) {
                    VRDriverInput()->UpdateBooleanComponent(m_proximity, true, 0.0);
                }
            }
            std::this_thread::sleep_for(std::chrono::milliseconds(2));
        }
    }

    std::atomic<uint32_t> m_deviceIndex{k_unTrackedDeviceIndexInvalid};
    std::atomic<bool> m_running{false};
    std::thread m_poseThread;
    VRInputComponentHandle_t m_proximity = k_ulInvalidInputComponentHandle;
    VRInputComponentHandle_t m_systemButton = k_ulInvalidInputComponentHandle;

    int32_t m_windowX = 100;
    int32_t m_windowY = 100;
    uint32_t m_windowWidth = 1600;
    uint32_t m_windowHeight = 900;
    bool m_monoView = true;
    float m_horizontalFovDeg = 100.0f;
    float m_renderScale = 1.0f;
    float m_displayFrequency = 60.0f;
    float m_ipdMeters = 0.063f;
    float m_headHeightMeters = 0.0f;
};

// ---- driver provider --------------------------------------------------------

class MouseHmdProvider : public IServerTrackedDeviceProvider {
public:
    EVRInitError Init(IVRDriverContext *pDriverContext) override {
        VR_INIT_SERVER_DRIVER_CONTEXT(pDriverContext);

        MouseHmdConfig cfg;
        cfg.mouseDegPerPx = SettingFloat("mouseDegPerPx", 0.12f);
        cfg.keyDegPerSec = SettingFloat("keyDegPerSec", 60.0f);
        cfg.grabStyle = SettingBool("grabStyle", true);
        cfg.anyWindow = SettingBool("anyWindow", false);
        cfg.smoothingMs = SettingFloat("smoothingMs", 0.0f);
        if (!mousehmd_input_start(&cfg)) {
            // Without the input thread the headset would show a picture that can
            // never be turned, so refuse to load instead.
            Log("mousehmd: could not start the input thread\n");
            return VRInitError_Driver_Failed;
        }

        m_hmd.LoadSettings();
        if (!VRServerDriverHost()->TrackedDeviceAdded(kSerial, TrackedDeviceClass_HMD, &m_hmd)) {
            Log("mousehmd: TrackedDeviceAdded failed\n");
            mousehmd_input_stop();
            return VRInitError_Driver_Unknown;
        }
        Log("mousehmd: initialized\n");
        return VRInitError_None;
    }

    void Cleanup() override {
        m_hmd.StopPoseThread();
        mousehmd_input_stop();
        VR_CLEANUP_SERVER_DRIVER_CONTEXT();
    }

    const char *const *GetInterfaceVersions() override { return k_InterfaceVersions; }
    void RunFrame() override {}
    bool ShouldBlockStandbyMode() override { return false; }
    void EnterStandby() override {}
    void LeaveStandby() override {}

private:
    MouseHmd m_hmd;
};

static MouseHmdProvider g_provider;

// Called from the exported HmdDriverFactory in src/lib.rs.
extern "C" void *mousehmd_cpp_factory(const char *pInterfaceName, int *pReturnCode) {
    if (strcmp(IServerTrackedDeviceProvider_Version, pInterfaceName) == 0) {
        return &g_provider;
    }
    if (pReturnCode) {
        *pReturnCode = VRInitError_Init_InterfaceNotFound;
    }
    return nullptr;
}
